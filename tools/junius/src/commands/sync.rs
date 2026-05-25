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
const GENERATED_RPC_REQUIRES_RS: &str = "platform/src/generated/rpc_requires.rs";
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
    /// Parsed `plugin.toml`, for `[exposes.components]`/`[dependencies]` codegen.
    manifest: PluginManifest,
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

    let rpc_requires_rs = render_rpc_requires_rs(&plugins);
    match apply_file(
        Path::new(GENERATED_RPC_REQUIRES_RS),
        &rpc_requires_rs,
        args.dry_run,
    ) {
        Ok(kind) => changes.push(FileChange {
            path: GENERATED_RPC_REQUIRES_RS.into(),
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

    if let Err(code) = apply_rpc_barrels(&plugins, args.dry_run, &mut changes) {
        return code;
    }
    if let Err(code) = apply_consumer_registries(&plugins, args.dry_run, &mut changes) {
        return code;
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

        let name = plugin.plugin.name.clone();
        let crate_name = format!("{name}-plugin");
        let module_name = crate_name.replace('-', "_");
        let struct_name = format!("{}Plugin", to_pascal_case(&name));

        out.push(ResolvedPlugin {
            name,
            crate_name,
            module_name,
            struct_name,
            manifest: plugin,
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

/// Render `platform/src/generated/rpc_requires.rs`: the `(service, method) ->
/// required permissions` table the host's RPC guard enforces, scanned from each
/// enabled plugin's `.proto` files.
fn render_rpc_requires_rs(plugins: &[ResolvedPlugin]) -> String {
    let mut entries: Vec<(String, String, Vec<String>)> = Vec::new();
    // (service_fqn -> plugin), for the host's per-request RPC ctx injection.
    let mut services: Vec<(String, String)> = Vec::new();
    for p in plugins {
        let proto_dir = PathBuf::from("plugins").join(&p.name).join("proto");
        let mut files = Vec::new();
        collect_proto_files(&proto_dir, &mut files);
        files.sort();
        for file in files {
            if let Ok(content) = std::fs::read_to_string(&file) {
                for svc in scan_proto_services(&content) {
                    services.push((svc, p.name.clone()));
                }
                entries.extend(scan_proto_requires(&content));
            }
        }
    }
    entries.sort();
    services.sort();
    services.dedup();

    let mut buf = String::with_capacity(512);
    buf.push_str(
        "// Generated by `junius sync` from <deployment>/platform.toml.\n\
         // DO NOT EDIT BY HAND. Re-runs of `junius sync` overwrite this file.\n\
         \n\
         /// `(service_fqn, method)` -> permission names required, read from each\n\
         /// method's `option (platform.requires)`. The host RPC guard enforces these.\n\
         pub static RPC_REQUIRES: &[(&str, &str, &[&str])] = &[\n",
    );
    for (service, method, perms) in &entries {
        let perms_lit = perms
            .iter()
            .map(|p| format!("\"{p}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(buf, "    (\"{service}\", \"{method}\", &[{perms_lit}]),");
    }
    buf.push_str("];\n\n");
    buf.push_str(
        "/// `service_fqn` -> owning plugin, so the host attaches the right\n\
         /// `PluginResourceCtx` per request on the shared `/rpc` router.\n\
         pub static RPC_SERVICES: &[(&str, &str)] = &[\n",
    );
    for (service, plugin) in &services {
        let _ = writeln!(buf, "    (\"{service}\", \"{plugin}\"),");
    }
    buf.push_str("];\n");
    buf
}

/// Extract every service's fully-qualified name (`package.Service`) from a proto.
#[allow(
    clippy::unwrap_used,
    reason = "compile-constant regexes are known-valid"
)]
pub(crate) fn scan_proto_services(content: &str) -> Vec<String> {
    let package = regex::Regex::new(r"(?m)^\s*package\s+([\w.]+)\s*;")
        .unwrap()
        .captures(content)
        .map(|c| c[1].to_string())
        .unwrap_or_default();
    let svc_re = regex::Regex::new(r"\bservice\s+(\w+)").unwrap();
    svc_re
        .captures_iter(content)
        .map(|c| {
            let name = &c[1];
            if package.is_empty() {
                name.to_string()
            } else {
                format!("{package}.{name}")
            }
        })
        .collect()
}

/// Recursively collect `*.proto` files under `dir`.
pub(crate) fn collect_proto_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_proto_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "proto") {
            out.push(path);
        }
    }
}

/// Extract `(service_fqn, method, perms)` for every method carrying
/// `option (platform.requires)`. Associates each option with the nearest
/// preceding `rpc`, and each `rpc` with the nearest preceding `service`, scoped
/// by the file's `package` — robust for conventionally-formatted protos.
#[allow(
    clippy::unwrap_used,
    reason = "compile-constant regexes are known-valid"
)]
pub(crate) fn scan_proto_requires(content: &str) -> Vec<(String, String, Vec<String>)> {
    let package = regex::Regex::new(r"(?m)^\s*package\s+([\w.]+)\s*;")
        .unwrap()
        .captures(content)
        .map(|c| c[1].to_string())
        .unwrap_or_default();
    let svc_re = regex::Regex::new(r"\bservice\s+(\w+)").unwrap();
    let rpc_re = regex::Regex::new(r"\brpc\s+(\w+)").unwrap();
    let req_re =
        regex::Regex::new(r#"option\s*\(\s*platform(?:\.v1)?\.requires\s*\)\s*=\s*"([^"]*)""#)
            .unwrap();

    let services: Vec<(usize, String)> = svc_re
        .captures_iter(content)
        .map(|c| (c.get(0).unwrap().start(), c[1].to_string()))
        .collect();
    let rpcs: Vec<(usize, String)> = rpc_re
        .captures_iter(content)
        .map(|c| (c.get(0).unwrap().start(), c[1].to_string()))
        .collect();

    let mut out = Vec::new();
    for cap in req_re.captures_iter(content) {
        let at = cap.get(0).unwrap().start();
        let Some((rpc_at, method)) = rpcs.iter().filter(|(o, _)| *o < at).next_back() else {
            continue;
        };
        let service = services
            .iter()
            .filter(|(o, _)| *o < *rpc_at)
            .next_back()
            .map(|(_, n)| n.clone())
            .unwrap_or_default();
        let fqn = if package.is_empty() {
            service
        } else {
            format!("{package}.{service}")
        };
        let perms: Vec<String> = cap[1]
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        out.push((fqn, method.clone(), perms));
    }
    out
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

fn rpc_barrel_path(plugin_name: &str) -> PathBuf {
    PathBuf::from(format!(
        "packages/generated/src/plugins/{plugin_name}/rpc.ts"
    ))
}

/// Whether a plugin ships any `.proto` files (and therefore has an RPC surface
/// and a generated barrel). UI-only plugins have none.
fn plugin_has_proto(plugin_name: &str) -> bool {
    let proto_dir = PathBuf::from("plugins").join(plugin_name).join("proto");
    let mut files = Vec::new();
    collect_proto_files(&proto_dir, &mut files);
    !files.is_empty()
}

/// Write each RPC-bearing plugin's barrel under
/// `packages/generated/src/plugins/<name>/rpc.ts`; UI-only plugins (no `proto/`)
/// have no generated `*_pb.ts` to re-export, so they are skipped.
fn apply_rpc_barrels(
    plugins: &[ResolvedPlugin],
    dry_run: bool,
    changes: &mut Vec<FileChange>,
) -> Result<(), i32> {
    for plugin in plugins {
        if !plugin_has_proto(&plugin.name) {
            continue;
        }
        let path = rpc_barrel_path(&plugin.name);
        let kind = apply_file(&path, &render_rpc_barrel(plugin), dry_run)?;
        changes.push(FileChange {
            path: path.display().to_string(),
            kind,
        });
    }
    Ok(())
}

/// Write each consumer plugin's `ComponentRegistry` augmentation into its own
/// frontend (`plugins/<name>/frontend/src/generated/`), where its `tsc` picks it
/// up. Only plugins whose declared dependencies expose components get one.
fn apply_consumer_registries(
    plugins: &[ResolvedPlugin],
    dry_run: bool,
    changes: &mut Vec<FileChange>,
) -> Result<(), i32> {
    for plugin in plugins {
        let Some(content) = render_consumer_registry_ts(plugin, plugins) else {
            continue;
        };
        let path = consumer_registry_path(&plugin.name);
        let kind = apply_file(&path, &content, dry_run)?;
        changes.push(FileChange {
            path: path.display().to_string(),
            kind,
        });
    }
    Ok(())
}

fn render_rpc_barrel(plugin: &ResolvedPlugin) -> String {
    // The plugin's protos live at plugins/<name>/proto/<name>/v1/... and
    // buf generate emits them under packages/generated/src/proto/<name>/v1/.
    // The barrel is the public surface plugin frontends (and the shell)
    // import; M09 narrows it per [dependencies.<dep>].rpc_methods.
    format!(
        "// Generated by `junius sync` from <deployment>/platform.toml.\n\
         // DO NOT EDIT BY HAND. Re-runs of `junius sync` overwrite this file.\n\
         //\n\
         // Re-exports of {plugin}'s generated RPC surface. The barrel exists\n\
         // so plugin consumers have a stable import path; M09 will narrow it\n\
         // to the methods declared in [dependencies.{plugin}].rpc_methods.\n\
         export * from '../../proto/{plugin}/v1/{plugin}_pb.js';\n",
        plugin = plugin.name,
    )
}

/// An exposed component, flattened across enabled plugins: the owning plugin's
/// name, the component name (the `[exposes.components.<name>]` key, also its
/// export from `@junius/plugin-<plugin>`), and a collision-proof import alias.
struct ExposedComponentRef {
    plugin: String,
    component: String,
    alias: String,
}

/// Every `[exposes.components]` entry across the enabled plugins, in
/// (plugin, component) order. The host registry imports each; per-consumer
/// augmentations reference the subset their dependencies expose.
fn exposed_components(plugins: &[ResolvedPlugin]) -> Vec<ExposedComponentRef> {
    let mut out = Vec::new();
    for p in plugins {
        for component in p.manifest.exposes.components.keys() {
            out.push(ExposedComponentRef {
                plugin: p.name.clone(),
                component: component.clone(),
                alias: format!("{}_{}", to_pascal_case(&p.name), component),
            });
        }
    }
    out
}

const GENERATED_TS_HEADER: &str = "// Generated by `junius sync` from <deployment>/platform.toml.\n\
     // DO NOT EDIT BY HAND. Re-runs of `junius sync` overwrite this file.\n";

/// Render the host's runtime component registry: import each enabled plugin's
/// exposed components and map `'<plugin>.<Component>'` → the component. Consumed
/// by `main.tsx` and handed to `ComponentRegistryProvider`. (The *types* a
/// consumer sees come from its per-consumer augmentation; see
/// [`render_consumer_registry_ts`].)
fn render_component_registry_ts(plugins: &[ResolvedPlugin]) -> String {
    let components = exposed_components(plugins);
    let mut buf = String::with_capacity(512);
    buf.push_str(GENERATED_TS_HEADER);
    buf.push_str(
        "//\n\
         // The runtime registry, populated from each enabled plugin's\n\
         // `[exposes.components]`. Per-key types come from each consumer's\n\
         // generated `ComponentRegistry` augmentation.\n\n",
    );
    buf.push_str("import type { ComponentRegistryValue } from '@junius/sdk';\n");
    for c in &components {
        let _ = writeln!(
            buf,
            "import {{ {comp} as {alias} }} from '@junius/plugin-{plugin}';",
            comp = c.component,
            alias = c.alias,
            plugin = c.plugin,
        );
    }
    buf.push('\n');
    if components.is_empty() {
        buf.push_str("export const componentRegistry: ComponentRegistryValue = {};\n");
    } else {
        buf.push_str("export const componentRegistry: ComponentRegistryValue = {\n");
        for c in &components {
            let _ = writeln!(buf, "  '{}.{}': {},", c.plugin, c.component, c.alias);
        }
        buf.push_str("};\n");
    }
    buf
}

/// Path of a consumer plugin's generated `ComponentRegistry` type augmentation.
fn consumer_registry_path(plugin_name: &str) -> PathBuf {
    PathBuf::from(format!(
        "plugins/{plugin_name}/frontend/src/generated/component-registry.ts"
    ))
}

/// Render a consumer plugin's `declare module '@junius/sdk'` augmentation of
/// `ComponentRegistry`: one `'<dep>.<Component>'` key per component exposed by a
/// plugin this consumer declares a `[dependencies.<dep>]` on. This is what makes
/// `useComponent('<dep>.<Component>')` type-check (and a typo or an undeclared
/// cross-plugin component a compile error) inside the consumer. Returns `None`
/// when the consumer's dependencies expose no components.
fn render_consumer_registry_ts(
    consumer: &ResolvedPlugin,
    plugins: &[ResolvedPlugin],
) -> Option<String> {
    let by_name: std::collections::HashMap<&str, &ResolvedPlugin> =
        plugins.iter().map(|p| (p.name.as_str(), p)).collect();

    let mut keys: Vec<(String, String)> = Vec::new();
    for dep_name in consumer.manifest.dependencies.keys() {
        if let Some(dep) = by_name.get(dep_name.as_str()) {
            for component in dep.manifest.exposes.components.keys() {
                keys.push((dep.name.clone(), component.clone()));
            }
        }
    }
    if keys.is_empty() {
        return None;
    }
    keys.sort();

    let mut buf = String::with_capacity(512);
    buf.push_str(GENERATED_TS_HEADER);
    buf.push_str(
        "//\n\
         // Augments `@junius/sdk`'s `ComponentRegistry` with the components this\n\
         // plugin's declared dependencies expose, so `useComponent(key)` is keyed\n\
         // and typed. `export {}` keeps this a module (so `declare module` merges).\n\n\
         export {};\n\n\
         declare module '@junius/sdk' {\n  \
         interface ComponentRegistry {\n",
    );
    for (plugin, component) in &keys {
        let _ = writeln!(
            buf,
            "    '{plugin}.{component}': typeof import('@junius/plugin-{plugin}').{component};",
        );
    }
    buf.push_str("  }\n}\n");
    Some(buf)
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

    /// Build a `ResolvedPlugin` from a `plugin.toml` body, deriving the crate /
    /// module / struct names the way `resolve_plugins` does.
    fn resolved(toml: &str) -> ResolvedPlugin {
        let manifest = PluginManifest::parse(toml).unwrap();
        let name = manifest.plugin.name.clone();
        let crate_name = format!("{name}-plugin");
        let module_name = crate_name.replace('-', "_");
        let struct_name = format!("{}Plugin", to_pascal_case(&name));
        ResolvedPlugin {
            name,
            crate_name,
            module_name,
            struct_name,
            manifest,
        }
    }

    #[test]
    fn pascal_case_basic() {
        assert_eq!(to_pascal_case("hello"), "Hello");
        assert_eq!(to_pascal_case("hello-world"), "HelloWorld");
        assert_eq!(to_pascal_case("speakers"), "Speakers");
        assert_eq!(to_pascal_case("user_admin"), "UserAdmin");
    }

    #[test]
    fn scan_proto_requires_extracts_annotated_methods() {
        let proto = r#"
            syntax = "proto3";
            package hello.v1;
            import "platform/v1/annotations.proto";
            service HelloService {
              rpc Greet(GreetRequest) returns (GreetResponse);
              rpc ListGreetings(L) returns (R) {
                option (platform.v1.requires) = "hello:read";
              }
              rpc CreateGreeting(C) returns (D) {
                option (platform.v1.requires) = "hello:read,hello:write";
              }
            }
        "#;
        let mut found = scan_proto_requires(proto);
        found.sort();
        assert_eq!(
            found,
            vec![
                (
                    "hello.v1.HelloService".to_string(),
                    "CreateGreeting".to_string(),
                    vec!["hello:read".to_string(), "hello:write".to_string()],
                ),
                (
                    "hello.v1.HelloService".to_string(),
                    "ListGreetings".to_string(),
                    vec!["hello:read".to_string()],
                ),
            ]
        );
    }

    #[test]
    fn plugins_rs_empty() {
        let out = render_plugins_rs(&[]);
        assert!(out.contains("Vec::new()"));
        assert!(out.contains("Generated by `junius sync`"));
    }

    const HELLO_MINIMAL: &str =
        "[plugin]\nname = \"hello\"\ndisplay_name = \"Hello\"\nmanifest_schema = 1\n";

    #[test]
    fn plugins_rs_one_entry() {
        let out = render_plugins_rs(&[resolved(HELLO_MINIMAL)]);
        assert!(out.contains("Box::new(hello_plugin::HelloPlugin::new()),"));
        assert!(out.contains("pub fn plugins()"));
    }

    #[test]
    fn platform_cargo_lines_format() {
        let lines = platform_cargo_lines(&[resolved(HELLO_MINIMAL)]);
        assert_eq!(
            lines,
            vec!["hello-plugin = { path = \"../plugins/hello\" }".to_string()]
        );
    }

    #[test]
    fn component_registry_imports_and_maps_exposed_components() {
        let hello = resolved(
            "[plugin]\nname = \"hello\"\ndisplay_name = \"Hello\"\nmanifest_schema = 1\n\
             [exposes.components.GreeterCard]\nmodule = \"./lib/GreeterCard\"\n",
        );
        let widgets = resolved(
            "[plugin]\nname = \"widgets\"\ndisplay_name = \"Widgets\"\nmanifest_schema = 1\n\
             [exposes.components.VenuePicker]\nmodule = \"./lib/VenuePicker\"\n",
        );
        let out = render_component_registry_ts(&[hello, widgets]);
        assert!(
            out.contains(
                "import { GreeterCard as Hello_GreeterCard } from '@junius/plugin-hello';"
            )
        );
        assert!(out.contains(
            "import { VenuePicker as Widgets_VenuePicker } from '@junius/plugin-widgets';"
        ));
        assert!(out.contains("'hello.GreeterCard': Hello_GreeterCard,"));
        assert!(out.contains("'widgets.VenuePicker': Widgets_VenuePicker,"));
    }

    #[test]
    fn component_registry_empty_when_nothing_exposed() {
        let out = render_component_registry_ts(&[resolved(HELLO_MINIMAL)]);
        assert!(out.contains("export const componentRegistry: ComponentRegistryValue = {};"));
    }

    #[test]
    fn consumer_registry_augments_for_declared_deps_only() {
        let hello = resolved(
            "[plugin]\nname = \"hello\"\ndisplay_name = \"Hello\"\nmanifest_schema = 1\n\
             [exposes.components.GreeterCard]\nmodule = \"./lib/GreeterCard\"\n",
        );
        let widgets = resolved(
            "[plugin]\nname = \"widgets\"\ndisplay_name = \"Widgets\"\nmanifest_schema = 1\n\
             [exposes.components.VenuePicker]\nmodule = \"./lib/VenuePicker\"\n",
        );
        // greetings depends on both hello and widgets.
        let greetings = resolved(
            "[plugin]\nname = \"greetings\"\ndisplay_name = \"Greetings\"\nmanifest_schema = 1\n\
             [dependencies.hello]\noptional = false\n[dependencies.widgets]\noptional = true\n",
        );
        let plugins = vec![hello.clone(), widgets.clone(), greetings.clone()];

        let out = render_consumer_registry_ts(&greetings, &plugins).unwrap();
        assert!(out.contains("declare module '@junius/sdk'"));
        assert!(
            out.contains("'hello.GreeterCard': typeof import('@junius/plugin-hello').GreeterCard;")
        );
        assert!(out.contains(
            "'widgets.VenuePicker': typeof import('@junius/plugin-widgets').VenuePicker;"
        ));

        // A plugin with no component-exposing deps gets no augmentation.
        assert!(render_consumer_registry_ts(&hello, &plugins).is_none());
    }
}
