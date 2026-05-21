//! `junius sync` — bring derived files in line with the deployment config.
//!
//! For M03 sync handles:
//! - `platform/src/generated/plugins.rs` — registry construction.
//! - `platform/Cargo.toml` managed region — `path` deps on each plugin crate.
//! - Workspace `Cargo.toml` managed region — `plugins/<name>` members.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use junius_manifest::{PlatformManifest, PluginManifest};
use serde::Serialize;

use crate::cli::{OutputFormat, SyncArgs};
use crate::exit;
use crate::markers;
use crate::output::{self, Renderable};

const GENERATED_PLUGINS_RS: &str = "platform/src/generated/plugins.rs";
const PLATFORM_CARGO_TOML: &str = "platform/Cargo.toml";
const WORKSPACE_CARGO_TOML: &str = "Cargo.toml";
const GENERATED_ROUTES_TS: &str = "platform/frontend/src/generated/routes.ts";
const GENERATED_REGISTRY_TS: &str = "platform/frontend/src/generated/component-registry.ts";
const DEFAULT_CONFIG: &str = "platform.toml";

/// One plugin to register, in declaration order.
#[derive(Debug, Clone)]
struct ResolvedPlugin {
    /// Plugin name from `plugin.toml` (`[plugin].name`).
    name: String,
    /// Cargo crate name. Convention: `<name>-plugin`.
    crate_name: String,
    /// Rust module name (Cargo's `s/-/_/g` rewrite of `crate_name`).
    module_name: String,
    /// Pascal-case struct name. Convention: `<PascalCase(name)>Plugin`.
    struct_name: String,
}

/// JSON shape of `junius sync`'s result.
#[derive(Serialize)]
struct SyncResult {
    config: String,
    plugins: Vec<String>,
    dry_run: bool,
    changes: Vec<FileChange>,
}

#[derive(Serialize)]
struct FileChange {
    path: String,
    /// `"created"`, `"updated"`, or `"unchanged"`.
    kind: &'static str,
}

impl Renderable for SyncResult {
    fn render_plain(&self, w: &mut dyn std::io::Write) -> std::io::Result<()> {
        if self.dry_run {
            writeln!(w, "Sync (dry-run) against {}:", self.config)?;
        } else {
            writeln!(w, "Sync against {}:", self.config)?;
        }
        writeln!(w, "  enabled plugins: {}", self.plugins.join(", "))?;
        for c in &self.changes {
            writeln!(w, "  {:9} {}", c.kind, c.path)?;
        }
        Ok(())
    }
}

pub fn run(args: &SyncArgs, format: OutputFormat) -> i32 {
    if args.plugin.is_some() {
        eprintln!("junius: sync --plugin scope is not yet implemented (planned for M09)");
        return exit::NOT_IMPLEMENTED;
    }

    let config_path = args
        .config
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG));
    let manifest = match load_platform_manifest(&config_path) {
        Ok(m) => m,
        Err(code) => return code,
    };

    let plugins = match resolve_plugins(&manifest) {
        Ok(p) => p,
        Err(code) => return code,
    };

    let mut changes = Vec::new();

    let plugins_rs = render_plugins_rs(&plugins);
    match apply_file(Path::new(GENERATED_PLUGINS_RS), &plugins_rs, args.dry_run) {
        Ok(kind) => changes.push(FileChange {
            path: GENERATED_PLUGINS_RS.into(),
            kind,
        }),
        Err(code) => return code,
    }

    match apply_marker_update(
        Path::new(PLATFORM_CARGO_TOML),
        &platform_cargo_lines(&plugins),
        args.dry_run,
    ) {
        Ok(kind) => changes.push(FileChange {
            path: PLATFORM_CARGO_TOML.into(),
            kind,
        }),
        Err(code) => return code,
    }

    match apply_marker_update(
        Path::new(WORKSPACE_CARGO_TOML),
        &workspace_cargo_lines(&plugins),
        args.dry_run,
    ) {
        Ok(kind) => changes.push(FileChange {
            path: WORKSPACE_CARGO_TOML.into(),
            kind,
        }),
        Err(code) => return code,
    }

    let routes_ts = render_routes_ts(&plugins);
    match apply_file(Path::new(GENERATED_ROUTES_TS), &routes_ts, args.dry_run) {
        Ok(kind) => changes.push(FileChange {
            path: GENERATED_ROUTES_TS.into(),
            kind,
        }),
        Err(code) => return code,
    }

    let registry_ts = render_component_registry_ts(&plugins);
    match apply_file(Path::new(GENERATED_REGISTRY_TS), &registry_ts, args.dry_run) {
        Ok(kind) => changes.push(FileChange {
            path: GENERATED_REGISTRY_TS.into(),
            kind,
        }),
        Err(code) => return code,
    }

    let any_pending = changes.iter().any(|c| c.kind != "unchanged");

    let result = SyncResult {
        config: config_path.display().to_string(),
        plugins: plugins.iter().map(|p| p.name.clone()).collect(),
        dry_run: args.dry_run,
        changes,
    };
    let _ = output::emit(format, &result);

    // Dry-run convention: exit 1 if changes would be written, 0 if everything
    // is already in sync. Lets CI use `junius sync --dry-run` as a drift check.
    if args.dry_run && any_pending {
        return 1;
    }
    exit::OK
}

// ---------------------------------------------------------------------------
// Plumbing
// ---------------------------------------------------------------------------

fn load_platform_manifest(path: &Path) -> Result<PlatformManifest, i32> {
    let src = std::fs::read_to_string(path).map_err(|e| {
        eprintln!("junius: cannot read {}: {e}", path.display());
        exit::PARSE_ERROR
    })?;
    let manifest = PlatformManifest::parse(&src).map_err(|e| {
        eprintln!("junius: parse error in {}: {e}", path.display());
        exit::PARSE_ERROR
    })?;
    let report = manifest.validate();
    if !report.is_ok() {
        for issue in report.errors() {
            eprintln!(
                "junius: [error] {} at {}: {}",
                issue.code, issue.path, issue.message
            );
        }
        return Err(exit::VALIDATION);
    }
    Ok(manifest)
}

fn resolve_plugins(manifest: &PlatformManifest) -> Result<Vec<ResolvedPlugin>, i32> {
    let mut out = Vec::with_capacity(manifest.plugins.enabled.len());
    for name in &manifest.plugins.enabled {
        let plugin_toml = PathBuf::from("plugins").join(name).join("plugin.toml");
        let src = std::fs::read_to_string(&plugin_toml).map_err(|e| {
            eprintln!("junius: cannot read {}: {e}", plugin_toml.display());
            exit::PARSE_ERROR
        })?;
        let plugin = PluginManifest::parse(&src).map_err(|e| {
            eprintln!("junius: parse error in {}: {e}", plugin_toml.display());
            exit::PARSE_ERROR
        })?;
        let report = plugin.validate();
        if !report.is_ok() {
            for issue in report.errors() {
                eprintln!(
                    "junius: [error] {} at {}: {} ({})",
                    issue.code,
                    issue.path,
                    issue.message,
                    plugin_toml.display(),
                );
            }
            return Err(exit::VALIDATION);
        }

        let crate_name = format!("{}-plugin", plugin.plugin.name);
        let module_name = crate_name.replace('-', "_");
        let struct_name = format!("{}Plugin", to_pascal_case(&plugin.plugin.name));

        out.push(ResolvedPlugin {
            name: plugin.plugin.name,
            crate_name,
            module_name,
            struct_name,
        });
    }
    Ok(out)
}

fn to_pascal_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper_next = true;
    for c in s.chars() {
        if c == '-' || c == '_' {
            upper_next = true;
        } else if upper_next {
            out.extend(c.to_uppercase());
            upper_next = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn to_camel_case(s: &str) -> String {
    let pascal = to_pascal_case(s);
    let mut chars = pascal.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Render targets
// ---------------------------------------------------------------------------

fn render_plugins_rs(plugins: &[ResolvedPlugin]) -> String {
    let mut buf = String::with_capacity(512);
    buf.push_str(
        "// Generated by `junius sync` from <deployment>/platform.toml.\n\
         // DO NOT EDIT BY HAND. Re-runs of `junius sync` overwrite this file.\n\
         \n\
         use junius_sdk::Plugin;\n\
         \n",
    );
    buf.push_str("/// Constructed plugin registry, in deployment-declared order.\n");
    buf.push_str("#[must_use]\n");
    buf.push_str("pub fn plugins() -> Vec<Box<dyn Plugin>> {\n");
    if plugins.is_empty() {
        buf.push_str("    Vec::new()\n");
    } else {
        buf.push_str("    vec![\n");
        for p in plugins {
            let _ = writeln!(
                buf,
                "        Box::new({}::{}::new()),",
                p.module_name, p.struct_name
            );
        }
        buf.push_str("    ]\n");
    }
    buf.push_str("}\n");
    buf
}

fn render_routes_ts(plugins: &[ResolvedPlugin]) -> String {
    let mut buf = String::with_capacity(512);
    buf.push_str(
        "// Generated by `junius sync` from <deployment>/platform.toml.\n\
         // DO NOT EDIT BY HAND. Re-runs of `junius sync` overwrite this file.\n\
         \n\
         import { createRoute } from '@tanstack/react-router';\n\
         \n\
         import { rootRoute } from '../router/root.js';\n",
    );

    for p in plugins {
        let _ = writeln!(
            buf,
            "import {{ buildRoutes as build{pascal} }} from '@junius/plugin-{name}';",
            pascal = p.struct_name.trim_end_matches("Plugin"),
            name = p.name,
        );
    }

    buf.push_str("\nconst indexRoute = createRoute({\n  getParentRoute: () => rootRoute,\n  path: '/',\n  component: () => null,\n});\n\n");

    for p in plugins {
        let pascal = p.struct_name.trim_end_matches("Plugin");
        let camel = to_camel_case(&p.name);
        let _ = writeln!(buf, "const {camel}Parent = createRoute({{");
        let _ = writeln!(buf, "  getParentRoute: () => rootRoute,");
        let _ = writeln!(buf, "  path: '/p/{}',", p.name);
        let _ = writeln!(buf, "}});");
        let _ = writeln!(
            buf,
            "{camel}Parent.addChildren(build{pascal}({camel}Parent));\n"
        );
    }

    buf.push_str("export const routeTree = rootRoute.addChildren([\n  indexRoute,\n");
    for p in plugins {
        let camel = to_camel_case(&p.name);
        let _ = writeln!(buf, "  {camel}Parent,");
    }
    buf.push_str("]);\n");

    buf
}

fn render_component_registry_ts(_plugins: &[ResolvedPlugin]) -> String {
    // At M04 the registry is always empty; M09 introduces real cross-plugin
    // component sharing and this function will scan each plugin manifest's
    // [exposes.components] block and emit the corresponding imports.
    "// Generated by `junius sync` from <deployment>/platform.toml.\n\
     // DO NOT EDIT BY HAND. Re-runs of `junius sync` overwrite this file.\n\
     //\n\
     // Populated from each enabled plugin's `[exposes.components]` block.\n\
     // Empty at M04; the cross-plugin component-sharing flow lands in M09.\n\
     \n\
     import type { ComponentRegistry } from '@junius/sdk';\n\
     \n\
     export const componentRegistry: ComponentRegistry = {};\n"
        .to_string()
}

fn platform_cargo_lines(plugins: &[ResolvedPlugin]) -> Vec<String> {
    plugins
        .iter()
        .map(|p| format!("{} = {{ path = \"../plugins/{}\" }}", p.crate_name, p.name))
        .collect()
}

fn workspace_cargo_lines(plugins: &[ResolvedPlugin]) -> Vec<String> {
    plugins
        .iter()
        .map(|p| format!("\"plugins/{}\",", p.name))
        .collect()
}

// ---------------------------------------------------------------------------
// File application
// ---------------------------------------------------------------------------

fn apply_file(path: &Path, new_content: &str, dry_run: bool) -> Result<&'static str, i32> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    if existing == new_content {
        return Ok("unchanged");
    }
    let kind = if existing.is_empty() {
        "created"
    } else {
        "updated"
    };
    if !dry_run {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    eprintln!("junius: cannot create {}: {e}", parent.display());
                    exit::PARSE_ERROR
                })?;
            }
        }
        std::fs::write(path, new_content).map_err(|e| {
            eprintln!("junius: cannot write {}: {e}", path.display());
            exit::PARSE_ERROR
        })?;
    }
    Ok(kind)
}

fn apply_marker_update(path: &Path, body: &[String], dry_run: bool) -> Result<&'static str, i32> {
    let existing = std::fs::read_to_string(path).map_err(|e| {
        eprintln!("junius: cannot read {}: {e}", path.display());
        exit::PARSE_ERROR
    })?;
    let new_content = markers::replace_managed(&existing, body).map_err(|e| {
        eprintln!("junius: failed to update {} markers: {e}", path.display());
        exit::PARSE_ERROR
    })?;
    if existing == new_content {
        return Ok("unchanged");
    }
    if !dry_run {
        std::fs::write(path, &new_content).map_err(|e| {
            eprintln!("junius: cannot write {}: {e}", path.display());
            exit::PARSE_ERROR
        })?;
    }
    Ok("updated")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn pascal_case_basic() {
        assert_eq!(to_pascal_case("hello"), "Hello");
        assert_eq!(to_pascal_case("hello-world"), "HelloWorld");
        assert_eq!(to_pascal_case("speakers"), "Speakers");
        assert_eq!(to_pascal_case("user_admin"), "UserAdmin");
    }

    #[test]
    fn plugins_rs_empty() {
        let out = render_plugins_rs(&[]);
        assert!(out.contains("Vec::new()"));
        assert!(out.contains("Generated by `junius sync`"));
    }

    #[test]
    fn plugins_rs_one_entry() {
        let out = render_plugins_rs(&[ResolvedPlugin {
            name: "hello".into(),
            crate_name: "hello-plugin".into(),
            module_name: "hello_plugin".into(),
            struct_name: "HelloPlugin".into(),
        }]);
        assert!(out.contains("Box::new(hello_plugin::HelloPlugin::new()),"));
        assert!(out.contains("pub fn plugins()"));
    }

    #[test]
    fn platform_cargo_lines_format() {
        let plugins = vec![ResolvedPlugin {
            name: "hello".into(),
            crate_name: "hello-plugin".into(),
            module_name: "hello_plugin".into(),
            struct_name: "HelloPlugin".into(),
        }];
        let lines = platform_cargo_lines(&plugins);
        assert_eq!(
            lines,
            vec!["hello-plugin = { path = \"../plugins/hello\" }".to_string()]
        );
    }
}
