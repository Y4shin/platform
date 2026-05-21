//! `junius check` — parse and validate manifests.

use std::path::{Path, PathBuf};

use junius_manifest::{
    ManifestError, PlatformManifest, PluginManifest, Severity, ValidationIssue, ValidationReport,
};
use miette::{Diagnostic, NamedSource, Report, SourceSpan};
use serde::Serialize;

use crate::cli::{CheckArgs, OutputFormat};
use crate::exit;
use crate::output::{self, Renderable};

/// JSON-shaped result of `junius check`.
#[derive(Serialize)]
struct CheckResult {
    ok: bool,
    path: String,
    issues: Vec<IssueJson>,
}

#[derive(Serialize)]
struct IssueJson {
    severity: &'static str,
    code: &'static str,
    path: String,
    message: String,
}

impl Renderable for CheckResult {
    fn render_plain(&self, w: &mut dyn std::io::Write) -> std::io::Result<()> {
        if self.ok {
            writeln!(w, "OK: {}", self.path)
        } else {
            writeln!(w, "FAIL: {} ({} issue(s))", self.path, self.issues.len())?;
            for issue in &self.issues {
                writeln!(
                    w,
                    "  [{}] {} at {}: {}",
                    issue.severity, issue.code, issue.path, issue.message
                )?;
            }
            Ok(())
        }
    }
}

pub fn run(args: &CheckArgs, format: OutputFormat) -> i32 {
    let path = match resolve_manifest_path(args) {
        Ok(Some(p)) => p,
        Ok(None) => {
            // Neither --manifest nor --plugin given. M01 treats this as a no-op.
            let result = CheckResult {
                ok: true,
                path: "<none>".into(),
                issues: vec![],
            };
            let _ = output::emit(format, &result);
            return exit::OK;
        }
        Err(code) => return code,
    };

    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("junius: cannot read {}: {e}", path.display());
            return exit::PARSE_ERROR;
        }
    };

    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("manifest.toml");

    match file_name {
        "plugin.toml" => check_plugin(&path, &src, format),
        "platform.toml" => check_platform(&path, &src, format),
        other => {
            eprintln!(
                "junius: don't know how to validate {other:?}; expected plugin.toml or platform.toml"
            );
            exit::PARSE_ERROR
        }
    }
}

fn resolve_manifest_path(args: &CheckArgs) -> Result<Option<PathBuf>, i32> {
    if let Some(p) = &args.manifest {
        return Ok(Some(p.clone()));
    }
    if let Some(name) = &args.plugin {
        let p = PathBuf::from("plugins").join(name).join("plugin.toml");
        if !p.exists() {
            eprintln!("junius: no manifest found at {}", p.display());
            return Err(exit::PARSE_ERROR);
        }
        return Ok(Some(p));
    }
    Ok(None)
}

fn check_plugin(path: &Path, src: &str, format: OutputFormat) -> i32 {
    match PluginManifest::parse(src) {
        Ok(manifest) => report(path, src, &manifest.validate(), format),
        Err(e) => emit_parse_error(path, src, &e, format),
    }
}

fn check_platform(path: &Path, src: &str, format: OutputFormat) -> i32 {
    match PlatformManifest::parse(src) {
        Ok(manifest) => report(path, src, &manifest.validate(), format),
        Err(e) => emit_parse_error(path, src, &e, format),
    }
}

fn report(path: &Path, src: &str, report: &ValidationReport, format: OutputFormat) -> i32 {
    let issues: Vec<IssueJson> = report
        .issues
        .iter()
        .map(|i| IssueJson {
            severity: match i.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            },
            code: i.code,
            path: i.path.clone(),
            message: i.message.clone(),
        })
        .collect();

    let ok = report.is_ok();
    let result = CheckResult {
        ok,
        path: path.display().to_string(),
        issues,
    };

    match format {
        OutputFormat::Json => {
            // JSON path: emit machine-readable shape; no miette rendering.
            let _ = output::emit(format, &result);
        }
        OutputFormat::Plain => {
            if ok {
                let _ = output::emit(format, &result);
            } else {
                // Pretty-render each issue via miette into stderr.
                let file_name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("manifest.toml")
                    .to_string();
                for issue in &report.issues {
                    let diag = issue_diagnostic(&file_name, src, issue);
                    eprintln!("{:?}", Report::new(diag));
                }
                eprintln!(
                    "FAIL: {} ({} issue(s))",
                    path.display(),
                    report.issues.len()
                );
            }
        }
    }

    if ok { exit::OK } else { exit::VALIDATION }
}

fn emit_parse_error(path: &Path, _src: &str, e: &ManifestError, format: OutputFormat) -> i32 {
    match format {
        OutputFormat::Json => {
            let result = CheckResult {
                ok: false,
                path: path.display().to_string(),
                issues: vec![IssueJson {
                    severity: "error",
                    code: "PARSE.ERROR",
                    path: "<root>".into(),
                    message: e.to_string(),
                }],
            };
            let _ = output::emit(format, &result);
        }
        OutputFormat::Plain => {
            eprintln!("junius: parse error in {}: {e}", path.display());
        }
    }
    exit::PARSE_ERROR
}

// --- miette diagnostic adapter ------------------------------------------------

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
struct CheckDiagnostic {
    code: String,
    message: String,
    src: NamedSource<String>,
    span: Option<SourceSpan>,
}

impl Diagnostic for CheckDiagnostic {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(self.code.clone()))
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        Some(&self.src)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        self.span.as_ref().map(|span| {
            let labeled = miette::LabeledSpan::new_with_span(Some("here".into()), *span);
            Box::new(std::iter::once(labeled)) as Box<dyn Iterator<Item = miette::LabeledSpan>>
        })
    }
}

fn issue_diagnostic(file_name: &str, src: &str, issue: &ValidationIssue) -> CheckDiagnostic {
    CheckDiagnostic {
        code: issue.code.into(),
        message: format!("{} ({})", issue.message, issue.path),
        src: NamedSource::new(file_name, src.to_string()),
        span: None,
    }
}
