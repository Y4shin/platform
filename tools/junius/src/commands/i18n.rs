//! `junius i18n check` — validate every enabled plugin's `i18n/*.po` catalogs
//! end-to-end without going through `cargo build`.
//!
//! Covers both flavours of catalog:
//!
//! - **Backend (key-based gettext):** `plugins/<name>/i18n/<locale>.po` +
//!   `platform/i18n/<locale>.po`. msgid is a stable catalog key
//!   (`event.signup.subject`); msgstr is the source-language template. The
//!   build helper (`junius-i18n-build`) is the canonical validator; we reuse
//!   it to check schema + placeholder agreement.
//! - **Frontend (Lingui source-string-as-msgid):**
//!   `plugins/<name>/frontend/i18n/<locale>.po` +
//!   `platform/frontend/i18n/<locale>.po`. msgid IS the English source
//!   string; msgstr is the translation. We check for untranslated entries and
//!   placeholder mismatches between msgid and each non-en msgstr.
//!
//! `--check-drift` additionally re-runs `lingui extract` and fails the
//! command if it would touch any `frontend/i18n/*.po` — that's the
//! "source has unwrapped strings, or someone deleted a wrapped one" gate.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use junius_i18n_build::ALL_LOCALES;
use junius_i18n_build::po;
use junius_manifest::PlatformManifest;

use crate::cli::{I18nCmd, OutputFormat};
use crate::exit;

pub fn run(cmd: &I18nCmd, _format: OutputFormat) -> i32 {
    match cmd {
        I18nCmd::Check {
            config,
            check_drift,
        } => check(config.as_deref(), *check_drift),
        I18nCmd::Extract { clean } => extract(*clean),
    }
}

/// Run `pnpm exec lingui extract` against the workspace. Lingui reads
/// `lingui.config.cjs` for the catalog set, walks each catalog's source
/// roots, and rewrites the corresponding `i18n/*.po`. Plugin authors invoke
/// this via `junius i18n extract` so they don't need Lingui CLI knowledge —
/// the wrapper means a future library swap stays in this file.
fn extract(clean: bool) -> i32 {
    let mut cmd = std::process::Command::new("pnpm");
    cmd.args(["exec", "lingui", "extract"]);
    if clean {
        cmd.arg("--clean");
    }
    match cmd.status() {
        Ok(status) if status.success() => exit::OK,
        Ok(status) => {
            eprintln!(
                "junius: i18n: extract failed (lingui exit {})",
                status.code().unwrap_or(-1)
            );
            exit::VALIDATION
        }
        Err(e) => {
            eprintln!("junius: i18n: cannot spawn pnpm: {e}");
            exit::PARSE_ERROR
        }
    }
}

fn check(config: Option<&Path>, check_drift: bool) -> i32 {
    let config_path =
        absolutize(config.map_or_else(|| PathBuf::from("platform.toml"), Path::to_path_buf));
    let src = match std::fs::read_to_string(&config_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("junius: i18n: read {}: {e}", config_path.display());
            return exit::PARSE_ERROR;
        }
    };
    let manifest = match PlatformManifest::parse(&src) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("junius: i18n: parse {}: {e}", config_path.display());
            return exit::PARSE_ERROR;
        }
    };

    // i18n catalogs live in the local source tree under `plugins/<name>/i18n/`
    // and `platform/i18n/` (BE) plus their `frontend/i18n/` siblings (FE).
    let source_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let mut failures = 0_u32;
    let mut total = 0_u32;

    // --- BE catalogs (key-based) --------------------------------------------
    let mut be_roots: Vec<(String, PathBuf)> = Vec::new();
    let host_be = source_root.join("platform").join("i18n");
    if host_be.exists() {
        be_roots.push(("platform".into(), host_be));
    }
    for name in &manifest.plugins.enabled {
        let dir = source_root.join("plugins").join(name).join("i18n");
        if dir.exists() {
            be_roots.push((name.clone(), dir));
        }
    }
    for (domain, dir) in &be_roots {
        total += 1;
        match validate_be(domain, dir) {
            Ok(count) => {
                println!("junius: i18n: {domain} (BE): ok ({count} messages)");
            }
            Err(msg) => {
                eprintln!("junius: i18n: {domain} (BE): FAIL\n  {msg}");
                failures += 1;
            }
        }
    }

    // --- FE catalogs (Lingui source-string-as-msgid) ------------------------
    let mut fe_roots: Vec<(String, PathBuf)> = Vec::new();
    let host_frontend = source_root.join("platform").join("frontend").join("i18n");
    if host_frontend.exists() {
        fe_roots.push(("platform".into(), host_frontend));
    }
    for name in &manifest.plugins.enabled {
        let dir = source_root
            .join("plugins")
            .join(name)
            .join("frontend")
            .join("i18n");
        if dir.exists() {
            fe_roots.push((name.clone(), dir));
        }
    }
    for (domain, dir) in &fe_roots {
        total += 1;
        match validate_fe(dir) {
            Ok(count) => {
                println!("junius: i18n: {domain} (FE): ok ({count} messages)");
            }
            Err(msg) => {
                eprintln!("junius: i18n: {domain} (FE): FAIL\n  {msg}");
                failures += 1;
            }
        }
    }

    // --- Drift gate ---------------------------------------------------------
    if check_drift && let Err(msg) = check_extract_drift() {
        eprintln!("junius: i18n: drift FAIL\n  {msg}");
        failures += 1;
    }

    if total == 0 && !check_drift {
        println!(
            "junius: i18n: no catalogs found (no plugin under `plugins/*/i18n/` and no `platform/i18n/`)"
        );
        return exit::OK;
    }

    if failures > 0 {
        eprintln!("junius: i18n: {failures} check(s) failed");
        exit::VALIDATION
    } else {
        println!("junius: i18n: all {total} catalog(s) ok");
        exit::OK
    }
}

/// Run the build helper's parse + schema + per-locale validation against a
/// BE (key-based) catalog; return the discovered message count on success.
fn validate_be(domain: &str, i18n_dir: &Path) -> Result<usize, String> {
    let out_dir =
        std::env::temp_dir().join(format!("junius-i18n-check-{}-{domain}", std::process::id()));
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("create temp dir: {e}"))?;
    let out_path = out_dir.join(format!("{domain}.rs"));

    let result = junius_i18n_build::generate_at(domain, ALL_LOCALES, i18n_dir, &out_path);
    let _ = std::fs::remove_file(&out_path);
    let _ = std::fs::remove_dir(&out_dir);

    result.map_err(|e| e.to_string())?;
    Ok(count_be_messages(i18n_dir))
}

/// Lingui-style FE catalog validation: every non-en `.po` must have all
/// msgstrs translated and use exactly the placeholders of the en msgid.
fn validate_fe(i18n_dir: &Path) -> Result<usize, String> {
    let en_path = i18n_dir.join("en.po");
    let en_src = std::fs::read_to_string(&en_path)
        .map_err(|e| format!("read {}: {e}", en_path.display()))?;
    let en_entries = po::parse(&en_src).map_err(|e| format!("parse {}: {e}", en_path.display()))?;

    // Build msgid → expected placeholder set from the source catalog.
    let mut expected = std::collections::BTreeMap::new();
    for entry in &en_entries {
        expected.insert(entry.msgid.as_str(), placeholders(&entry.msgid));
    }

    let mut errors = Vec::new();
    for locale in ALL_LOCALES {
        if *locale == "en" || *locale == "pseudo" {
            // en is the source; pseudo is auto-filled by Lingui at compile time.
            continue;
        }
        let path = i18n_dir.join(format!("{locale}.po"));
        if !path.exists() {
            continue;
        }
        let src =
            std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let entries = po::parse(&src).map_err(|e| format!("parse {}: {e}", path.display()))?;
        for entry in entries {
            if entry.msgstr.trim().is_empty() {
                errors.push(format!(
                    "{}:{}: msgid {:?} is untranslated in {locale}",
                    path.display(),
                    entry.line,
                    entry.msgid
                ));
                continue;
            }
            // A msgid in a translation file that isn't in en isn't an error
            // (older entry, will be cleaned by `lingui extract --clean`); skip.
            let Some(want) = expected.get(entry.msgid.as_str()) else {
                continue;
            };
            let got = placeholders(&entry.msgstr);
            let extra: Vec<&str> = got.difference(want).copied().collect();
            if !extra.is_empty() {
                errors.push(format!(
                    "{}:{}: msgid {:?} in {locale} references unknown placeholder(s) {:?}",
                    path.display(),
                    entry.line,
                    entry.msgid,
                    extra
                ));
            }
        }
    }
    if !errors.is_empty() {
        return Err(errors.join("\n  "));
    }
    Ok(en_entries.len())
}

/// Run `lingui extract` and report failure if any `.po` ends up different
/// from `HEAD`. CI assumption: working tree starts clean (fresh checkout);
/// any divergence from `HEAD` after extract = the committed catalogs don't
/// match source.
fn check_extract_drift() -> Result<(), String> {
    // Pre-check: the catalog files must match HEAD, otherwise we can't tell
    // user's in-progress edits from extract drift.
    let pre = std::process::Command::new("git")
        .args(["diff", "--quiet", "HEAD", "--", "**/i18n/*.po"])
        .status()
        .map_err(|e| format!("spawn git diff: {e}"))?;
    if !pre.success() {
        return Err(
            "i18n catalogs differ from HEAD before extract; commit/stash them and rerun".into(),
        );
    }
    let extract = std::process::Command::new("pnpm")
        .args(["exec", "lingui", "extract"])
        .status()
        .map_err(|e| format!("spawn lingui extract: {e}"))?;
    if !extract.success() {
        return Err(format!(
            "lingui extract failed (exit {})",
            extract.code().unwrap_or(-1)
        ));
    }
    let diff = std::process::Command::new("git")
        .args(["diff", "--exit-code", "HEAD", "--", "**/i18n/*.po"])
        .status()
        .map_err(|e| format!("spawn git diff: {e}"))?;
    if diff.success() {
        println!("junius: i18n: drift: ok (catalogs match source)");
        Ok(())
    } else {
        Err(
            "`lingui extract` modified catalog files — source has unwrapped strings (or deletions). Run `junius i18n extract` locally + commit the result."
                .into(),
        )
    }
}

/// Count msgids in `en.po` for the BE success-line summary.
fn count_be_messages(i18n_dir: &Path) -> usize {
    std::fs::read_to_string(i18n_dir.join("en.po"))
        .map(|s| {
            s.lines()
                .filter(|l| l.trim_start().starts_with("msgid "))
                .count()
        })
        .unwrap_or(0)
        // Don't count the gettext header entry (`msgid ""`).
        .saturating_sub(
            std::fs::read_to_string(i18n_dir.join("en.po"))
                .ok()
                .map_or(0, |s| usize::from(s.contains("msgid \"\""))),
        )
}

/// Extract the set of top-level `{placeholder}` names from a Lingui-style
/// message. Lingui placeholders are `{name}`; ICU plurals/selects are
/// `{name, plural, ...}` (the leading `name` is the variable, the rest is
/// the form description we don't care about for matching). `{{` / `}}`
/// escape literal braces.
///
/// Brace balancing is required to skip over the nested `{...}` shells of
/// ICU forms.
fn placeholders(s: &str) -> BTreeSet<&str> {
    let mut out = BTreeSet::new();
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (_, c) = chars[i];
        if c != '{' {
            i += 1;
            continue;
        }
        // `{{` is a literal brace, skip.
        if chars.get(i + 1).is_some_and(|&(_, c)| c == '{') {
            i += 2;
            continue;
        }
        // Walk to the matching `}` respecting nesting depth.
        let start_byte = chars[i].0 + 1;
        let mut depth = 1;
        let mut j = i + 1;
        let mut end_byte = start_byte;
        while j < chars.len() && depth > 0 {
            let (_, cj) = chars[j];
            match cj {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end_byte = chars[j].0;
                    }
                }
                _ => {}
            }
            j += 1;
        }
        if depth == 0 && end_byte > start_byte {
            let body = &s[start_byte..end_byte];
            // `{name}` → "name"; `{name, plural, ...}` → "name".
            let head = body.split(',').next().unwrap_or(body).trim();
            if !head.is_empty() {
                out.insert(head);
            }
        }
        i = j;
    }
    out
}

fn absolutize(p: PathBuf) -> PathBuf {
    if p.is_absolute() {
        p
    } else {
        std::env::current_dir().map(|cwd| cwd.join(&p)).unwrap_or(p)
    }
}
