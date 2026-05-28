//! `junius sync` — bring derived files in line with the deployment config.
//!
//! For M03 sync handles:
//! - `platform/src/generated/plugins.rs` — registry construction.
//! - `platform/Cargo.toml` managed region — `path` deps on each plugin crate.
//! - Workspace `Cargo.toml` managed region — `plugins/<name>` members.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use junius_manifest::{PlatformManifest, PluginManifest};
use junius_rpc_meta::{
    collect_proto_files, rustfmt_str, scan_proto_requires, scan_proto_service_methods,
};
use serde::Serialize;

use crate::cli::OutputFormat;
#[cfg(feature = "develop")]
use crate::cli::SyncArgs;
use crate::exit;
use crate::markers;
use crate::output::{self, Renderable};

const GENERATED_PLUGINS_RS: &str = "platform/src/generated/plugins.rs";
const GENERATED_RPC_REQUIRES_RS: &str = "platform/src/generated/rpc_requires.rs";
const PLATFORM_CARGO_TOML: &str = "platform/Cargo.toml";
const WORKSPACE_CARGO_TOML: &str = "Cargo.toml";
const BUF_YAML: &str = "buf.yaml";
const HOST_FRONTEND_PACKAGE_JSON: &str = "platform/frontend/package.json";
const GENERATED_ROUTES_TS: &str = "platform/frontend/src/generated/routes.ts";
const GENERATED_REGISTRY_TS: &str = "platform/frontend/src/generated/component-registry.ts";
const GENERATED_I18N_TS: &str = "platform/frontend/src/generated/i18n-catalogs.ts";
const GENERATED_DOMAINS_RS: &str = "crates/junius-sdk/src/generated/domains.rs";
#[cfg(feature = "develop")]
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
    /// Whether the plugin ships a `frontend/` package. Backend-only plugins
    /// (none) get no frontend wiring (routes/registry imports).
    has_frontend: bool,
    /// Whether the plugin ships Lingui catalogs (`frontend/i18n/en.po`
    /// exists). Drives the generated `i18n-catalogs.ts` import list.
    has_frontend_i18n: bool,
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

#[cfg(feature = "develop")]
pub fn run(args: &SyncArgs, format: OutputFormat) -> i32 {
    if args.plugin.is_some() {
        eprintln!("junius: sync --plugin scope is not yet implemented (planned for M09)");
        return exit::NOT_IMPLEMENTED;
    }
    if args.bundle_all {
        // M24: ignore `[plugins].enabled` (and `--config`) entirely. The
        // image build wants every monorepo plugin linked in regardless of
        // any deployment toml that may or may not exist at build time.
        return run_in_bundle_all(Path::new("."), args.dry_run, format);
    }
    let config_path = args
        .config
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG));
    // In-source `junius sync` runs with the source tree = the current directory.
    run_in(Path::new("."), &config_path, args.dry_run, format)
}

/// Generate composition glue for `config_path`'s enabled plugins **into**
/// `source_root` (the resolved platform source tree). `run` passes `.`; `build`
/// passes the resolved/cached source root so it can compose from a deployment
/// directory.
#[allow(
    clippy::too_many_lines,
    reason = "linear pipeline (manifest → resolve → generate → apply → subprocess); each step is short"
)]
pub(crate) fn run_in(
    source_root: &Path,
    config_path: &Path,
    dry_run: bool,
    format: OutputFormat,
) -> i32 {
    let manifest = match load_platform_manifest(config_path) {
        Ok(m) => m,
        Err(code) => return code,
    };

    let plugins = match resolve_plugins(&manifest, source_root) {
        Ok(p) => p,
        Err(code) => return code,
    };

    apply_codegen(
        source_root,
        &config_path.display().to_string(),
        &plugins,
        dry_run,
        format,
    )
}

/// M24 bundle-all variant of `run_in`: skip the deployment-toml read and
/// link every plugin under `<source_root>/plugins/` instead. Used by the
/// precompiled-image Docker builds.
fn run_in_bundle_all(source_root: &Path, dry_run: bool, format: OutputFormat) -> i32 {
    let plugins = match resolve_all_plugins(source_root) {
        Ok(p) => p,
        Err(code) => return code,
    };
    apply_codegen(source_root, "(bundle-all)", &plugins, dry_run, format)
}

#[allow(
    clippy::too_many_lines,
    reason = "linear pipeline (generate → apply → subprocess); each step is short"
)]
fn apply_codegen(
    source_root: &Path,
    config_label: &str,
    plugins: &[ResolvedPlugin],
    dry_run: bool,
    format: OutputFormat,
) -> i32 {
    let mut changes = Vec::new();

    // (relative-path constant, rendered content) — written under `source_root`,
    // but reported by their stable relative path.
    // The generated Rust is rustfmt-formatted *here* (on the rendered string,
    // before writing) so the on-disk file matches what `sync` renders — keeping
    // `cargo fmt --check` clean AND `sync --dry-run` drift-free.
    let files: [(&str, String); 6] = [
        (
            GENERATED_PLUGINS_RS,
            rustfmt_str(render_plugins_rs(plugins)),
        ),
        (
            GENERATED_RPC_REQUIRES_RS,
            rustfmt_str(render_rpc_requires_rs(plugins, source_root)),
        ),
        (GENERATED_ROUTES_TS, render_routes_ts(plugins)),
        (GENERATED_REGISTRY_TS, render_component_registry_ts(plugins)),
        (GENERATED_I18N_TS, render_i18n_catalogs_ts(plugins)),
        (
            GENERATED_DOMAINS_RS,
            rustfmt_str(render_domains_rs(plugins)),
        ),
    ];
    for (rel, content) in &files {
        match apply_file(&source_root.join(rel), content, dry_run) {
            Ok(kind) => changes.push(FileChange {
                path: (*rel).into(),
                kind,
            }),
            Err(code) => return code,
        }
    }

    let markers: [(&str, Vec<String>); 2] = [
        (PLATFORM_CARGO_TOML, platform_cargo_lines(plugins)),
        (WORKSPACE_CARGO_TOML, workspace_cargo_lines(plugins)),
    ];
    for (rel, body) in &markers {
        match apply_marker_update(&source_root.join(rel), body, dry_run) {
            Ok(kind) => changes.push(FileChange {
                path: (*rel).into(),
                kind,
            }),
            Err(code) => return code,
        }
    }

    // M16 item A: manage the proto-module list in `buf.yaml` (one entry per
    // enabled plugin with a `proto/` dir) and the `@junius/plugin-<name>`
    // entries in the host frontend's `package.json` (one per plugin with a
    // `frontend/` dir). Both are *optional* — a deployment without proto or
    // frontend wiring (e.g. some test fixtures) skips them silently.
    let buf_yaml_path = source_root.join(BUF_YAML);
    if buf_yaml_path.exists() {
        let body = buf_yaml_module_lines(plugins, source_root);
        match apply_marker_update(&buf_yaml_path, &body, dry_run) {
            Ok(kind) => changes.push(FileChange {
                path: BUF_YAML.into(),
                kind,
            }),
            Err(code) => return code,
        }
    }
    let pkg_json_path = source_root.join(HOST_FRONTEND_PACKAGE_JSON);
    if pkg_json_path.exists() {
        match apply_host_frontend_deps(&pkg_json_path, plugins, dry_run) {
            Ok(kind) => changes.push(FileChange {
                path: HOST_FRONTEND_PACKAGE_JSON.into(),
                kind,
            }),
            Err(code) => return code,
        }
    }

    if let Err(code) = apply_rpc_barrels(plugins, source_root, dry_run, &mut changes) {
        return code;
    }
    if let Err(code) = apply_consumer_registries(plugins, source_root, dry_run, &mut changes) {
        return code;
    }

    let any_pending = changes.iter().any(|c| c.kind != "unchanged");

    // M16 item A: run the codegen / dep-install subprocesses that downstream
    // typechecks depend on, but only when the corresponding managed file
    // actually changed (not every sync run). Skipped in dry-run mode so a
    // CI drift check stays read-only.
    if !dry_run {
        let buf_yaml_changed = changes
            .iter()
            .any(|c| c.path == BUF_YAML && c.kind != "unchanged");
        let pkg_json_changed = changes
            .iter()
            .any(|c| c.path == HOST_FRONTEND_PACKAGE_JSON && c.kind != "unchanged");
        if buf_yaml_changed {
            if let Err(code) = run_subprocess(
                "pnpm",
                &["exec", "buf", "generate"],
                source_root,
                "buf generate",
            ) {
                return code;
            }
        }
        if pkg_json_changed {
            if let Err(code) = run_subprocess("pnpm", &["install"], source_root, "pnpm install") {
                return code;
            }
        }
    }

    let result = SyncResult {
        config: config_label.to_string(),
        plugins: plugins.iter().map(|p| p.name.clone()).collect(),
        dry_run,
        changes,
    };
    let _ = output::emit(format, &result);

    // Dry-run convention: exit 1 if changes would be written, 0 if everything
    // is already in sync. Lets CI use `junius sync --dry-run` as a drift check.
    if dry_run && any_pending {
        return 1;
    }
    exit::OK
}

/// `<source_root>/plugins/<name>/proto` — where a plugin's `.proto` files live.
fn plugin_proto_dir(source_root: &Path, plugin_name: &str) -> PathBuf {
    source_root.join("plugins").join(plugin_name).join("proto")
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

fn resolve_plugins(
    manifest: &PlatformManifest,
    source_root: &Path,
) -> Result<Vec<ResolvedPlugin>, i32> {
    let mut out = Vec::with_capacity(manifest.plugins.enabled.len());
    for name in &manifest.plugins.enabled {
        out.push(resolve_plugin_by_name(name, source_root)?);
    }
    Ok(out)
}

/// Parse `<source_root>/plugins/<name>/plugin.toml`, validate it, and build
/// the [`ResolvedPlugin`] the codegen pipeline consumes. Shared between the
/// manifest-driven flow ([`resolve_plugins`]) and the M24 bundle-all flow
/// ([`resolve_all_plugins`]).
fn resolve_plugin_by_name(name: &str, source_root: &Path) -> Result<ResolvedPlugin, i32> {
    let plugin_toml = source_root.join("plugins").join(name).join("plugin.toml");
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
    let frontend_dir = source_root.join("plugins").join(&name).join("frontend");
    let has_frontend = frontend_dir.is_dir();
    let has_frontend_i18n = frontend_dir.join("i18n").join("en.po").is_file();
    Ok(ResolvedPlugin {
        name,
        crate_name,
        module_name,
        struct_name,
        manifest: plugin,
        has_frontend,
        has_frontend_i18n,
    })
}

/// M24: enumerate every plugin under `<source_root>/plugins/` that ships a
/// `plugin.toml`, sorted by directory name for deterministic codegen. The
/// bundle-all variant of `junius sync` consumes this in place of the
/// deployment's `[plugins].enabled` list.
fn enumerate_all_plugins(source_root: &Path) -> Result<Vec<String>, i32> {
    let plugins_dir = source_root.join("plugins");
    let entries = std::fs::read_dir(&plugins_dir).map_err(|e| {
        eprintln!("junius: cannot read {}: {e}", plugins_dir.display());
        exit::PARSE_ERROR
    })?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| {
            eprintln!("junius: reading plugins dir: {e}");
            exit::PARSE_ERROR
        })?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if !path.join("plugin.toml").is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        names.push(name.to_string());
    }
    names.sort();
    Ok(names)
}

fn resolve_all_plugins(source_root: &Path) -> Result<Vec<ResolvedPlugin>, i32> {
    let names = enumerate_all_plugins(source_root)?;
    let mut out = Vec::with_capacity(names.len());
    for name in &names {
        out.push(resolve_plugin_by_name(name, source_root)?);
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
fn render_rpc_requires_rs(plugins: &[ResolvedPlugin], source_root: &Path) -> String {
    let mut entries: Vec<(String, String, Vec<String>)> = Vec::new();
    // (service_fqn -> plugin), for the host's per-request RPC ctx injection.
    let mut services: Vec<(String, String)> = Vec::new();
    for p in plugins {
        let proto_dir = plugin_proto_dir(source_root, &p.name);
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

fn render_routes_ts(plugins: &[ResolvedPlugin]) -> String {
    // Only plugins that ship a `frontend/` package get route wiring; backend-only
    // plugins contribute nothing to the SPA.
    let fe: Vec<&ResolvedPlugin> = plugins.iter().filter(|p| p.has_frontend).collect();
    let publics: Vec<&ResolvedPlugin> = fe
        .iter()
        .copied()
        .filter(|p| p.manifest.plugin.public_routes)
        .collect();

    let mut buf = String::with_capacity(512);
    buf.push_str(
        "// Generated by `junius sync` from <deployment>/platform.toml.\n\
         // DO NOT EDIT BY HAND. Re-runs of `junius sync` overwrite this file.\n\
         \n\
         import { createRoute } from '@tanstack/react-router';\n\
         \n\
         import { authedLayoutRoute, rootRoute } from '../router/root.js';\n",
    );

    for p in &fe {
        let pascal = p.struct_name.trim_end_matches("Plugin");
        if p.manifest.plugin.public_routes {
            let _ = writeln!(
                buf,
                "import {{ buildRoutes as build{pascal}, buildPublicRoutes as build{pascal}Public }} from '@junius/plugin-{name}';",
                name = p.name,
            );
        } else {
            let _ = writeln!(
                buf,
                "import {{ buildRoutes as build{pascal} }} from '@junius/plugin-{name}';",
                name = p.name,
            );
        }
    }

    // Authed app: the index + every plugin's `/p/<name>` routes, wrapped by the
    // Shell via the pathless `authedLayoutRoute`.
    buf.push_str("\nconst indexRoute = createRoute({\n  getParentRoute: () => authedLayoutRoute,\n  path: '/',\n  component: () => null,\n});\n\n");

    for p in &fe {
        let pascal = p.struct_name.trim_end_matches("Plugin");
        let camel = to_camel_case(&p.name);
        let _ = writeln!(buf, "const {camel}Parent = createRoute({{");
        let _ = writeln!(buf, "  getParentRoute: () => authedLayoutRoute,");
        let _ = writeln!(buf, "  path: '/p/{}',", p.name);
        let _ = writeln!(buf, "}});");
        let _ = writeln!(
            buf,
            "{camel}Parent.addChildren(build{pascal}({camel}Parent));\n"
        );
    }

    // Public (login-optional) routes: each opted-in plugin's `/i/<name>` surface,
    // mounted off the bare root so it renders without the authed Shell.
    for p in &publics {
        let pascal = p.struct_name.trim_end_matches("Plugin");
        let camel = to_camel_case(&p.name);
        let _ = writeln!(buf, "const {camel}PublicParent = createRoute({{");
        let _ = writeln!(buf, "  getParentRoute: () => rootRoute,");
        let _ = writeln!(buf, "  path: '/i/{}',", p.name);
        let _ = writeln!(buf, "}});");
        let _ = writeln!(
            buf,
            "{camel}PublicParent.addChildren(build{pascal}Public({camel}PublicParent));\n"
        );
    }

    buf.push_str(
        "export const routeTree = rootRoute.addChildren([\n  authedLayoutRoute.addChildren([\n    indexRoute,\n",
    );
    for p in &fe {
        let camel = to_camel_case(&p.name);
        let _ = writeln!(buf, "    {camel}Parent,");
    }
    buf.push_str("  ]),\n");
    for p in &publics {
        let camel = to_camel_case(&p.name);
        let _ = writeln!(buf, "  {camel}PublicParent,");
    }
    buf.push_str("]);\n\n");

    // Path prefixes AuthProvider must not redirect away from (login-optional).
    if publics.is_empty() {
        buf.push_str("export const PUBLIC_ROUTE_PREFIXES: string[] = [];\n");
    } else {
        buf.push_str("export const PUBLIC_ROUTE_PREFIXES: string[] = [\n");
        for p in &publics {
            let _ = writeln!(buf, "  '/i/{}',", p.name);
        }
        buf.push_str("];\n");
    }

    buf
}

/// Emit `crates/junius-sdk/src/generated/domains.rs`: the `Domain` enum that
/// indexes per-plugin catalogs in the `Localizer`. `Platform = 0` is reserved
/// for the host's own catalog; each enabled plugin gets the next variant in
/// the order it appears in `[plugins].enabled`.
fn render_domains_rs(plugins: &[ResolvedPlugin]) -> String {
    let mut buf = String::with_capacity(1024);
    buf.push_str(
        "// Generated by `junius sync` from <deployment>/platform.toml.\n\
         // DO NOT EDIT BY HAND. Re-runs of `junius sync` overwrite this file.\n\
         \n\
         /// The set of installed plugin domains for this deployment. `Platform = 0`\n\
         /// is reserved for the host's own catalog (deployment-wide strings); the\n\
         /// remaining variants follow the order of `[plugins].enabled` in the\n\
         /// deployment `platform.toml`.\n\
         #[repr(usize)]\n\
         #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]\n\
         pub enum Domain {\n\
        \x20   Platform = 0,\n",
    );
    for (idx, p) in plugins.iter().enumerate() {
        let _ = writeln!(
            buf,
            "    {pascal} = {n},",
            pascal = to_pascal_case(&p.name),
            n = idx + 1,
        );
    }
    buf.push_str("}\n\n");

    let count = plugins.len() + 1;
    let _ = writeln!(buf, "impl Domain {{");
    let _ = writeln!(buf, "    pub const COUNT: usize = {count};");
    let _ = write!(
        buf,
        "    pub const ALL: [Domain; Self::COUNT] = [Self::Platform"
    );
    for p in plugins {
        let _ = write!(buf, ", Self::{}", to_pascal_case(&p.name));
    }
    buf.push_str("];\n\n");

    // name() — exhaustive match on every variant.
    buf.push_str("    /// Catalog domain name as it appears in `.po` headers and in the\n");
    buf.push_str("    /// build helper's `Options.domain` field.\n");
    buf.push_str("    #[must_use]\n");
    buf.push_str("    pub const fn name(self) -> &'static str {\n");
    buf.push_str("        match self {\n");
    buf.push_str("            Self::Platform => \"platform\",\n");
    for p in plugins {
        let _ = writeln!(
            buf,
            "            Self::{pascal} => {name:?},",
            pascal = to_pascal_case(&p.name),
            name = p.name,
        );
    }
    buf.push_str("        }\n    }\n\n");

    // from_name() — const-fn if-chain on byte slices.
    buf.push_str("    /// Parse a domain name. `None` for unknown names — callers should\n");
    buf.push_str("    /// treat that as a configuration error. `const` so the build-time\n");
    buf.push_str("    /// codegen can emit `const __DOMAIN: Domain = ...`.\n");
    buf.push_str("    #[must_use]\n");
    buf.push_str("    pub const fn from_name(s: &str) -> Option<Self> {\n");
    buf.push_str("        let b = s.as_bytes();\n");
    buf.push_str("        if matches_bytes(b, b\"platform\") {\n");
    buf.push_str("            Some(Self::Platform)\n");
    for p in plugins {
        let _ = writeln!(
            buf,
            "        }} else if matches_bytes(b, b{name:?}) {{",
            name = p.name,
        );
        let _ = writeln!(
            buf,
            "            Some(Self::{pascal})",
            pascal = to_pascal_case(&p.name),
        );
    }
    buf.push_str("        } else {\n");
    buf.push_str("            None\n");
    buf.push_str("        }\n    }\n}\n\n");

    buf.push_str(
        "/// `const`-context byte-slice equality. The standard `PartialEq` impl on\n\
         /// `&[u8]` is not yet `const`-callable; this small helper is.\n\
         const fn matches_bytes(a: &[u8], b: &[u8]) -> bool {\n\
        \x20   if a.len() != b.len() {\n\
        \x20       return false;\n\
        \x20   }\n\
        \x20   let mut i = 0;\n\
        \x20   while i < a.len() {\n\
        \x20       if a[i] != b[i] {\n\
        \x20           return false;\n\
        \x20       }\n\
        \x20       i += 1;\n\
        \x20   }\n\
        \x20   true\n\
         }\n",
    );
    buf
}

/// Emit `platform/frontend/src/generated/i18n-catalogs.ts`: the list of
/// `CatalogLoader`s the host passes to `<I18nProvider catalogs={...}>`. One
/// entry per FE plugin that ships an `i18n/` directory (Lingui catalogs).
/// The host shell's own catalog (`./i18n.ts → loadShellI18n`) stays
/// hand-managed; `main.tsx` concatenates the two sources.
fn render_i18n_catalogs_ts(plugins: &[ResolvedPlugin]) -> String {
    let with_i18n: Vec<&ResolvedPlugin> = plugins
        .iter()
        .filter(|p| p.has_frontend && p.has_frontend_i18n)
        .collect();

    let mut buf = String::with_capacity(512);
    buf.push_str(
        "// Generated by `junius sync` from <deployment>/platform.toml.\n\
         // DO NOT EDIT BY HAND. Re-runs of `junius sync` overwrite this file.\n\
         \n\
         import type { CatalogLoader } from '@junius/sdk';\n",
    );
    for p in &with_i18n {
        let camel = to_camel_case(&p.name);
        let pascal = p.struct_name.trim_end_matches("Plugin");
        let _ = writeln!(
            buf,
            "import {{ loadI18n as load{pascal}I18n }} from '@junius/plugin-{name}';",
            name = p.name,
        );
        // Force-include the camel name so unused-import lint doesn't fire if a
        // plugin lacks i18n — handled by the filter above, but defensive.
        let _ = camel;
    }
    buf.push_str("\n/**\n");
    buf.push_str(" * Per-plugin Lingui catalog loaders, in deployment-declared order. The host\n");
    buf.push_str(" * shell prepends its own `loadShellI18n` (see `./i18n.ts`) and passes the\n");
    buf.push_str(" * combined array to `<I18nProvider catalogs={...}>`.\n");
    buf.push_str(" */\n");
    if with_i18n.is_empty() {
        buf.push_str("export const PLUGIN_I18N_CATALOGS: readonly CatalogLoader[] = [];\n");
    } else {
        buf.push_str("export const PLUGIN_I18N_CATALOGS: readonly CatalogLoader[] = [\n");
        for p in &with_i18n {
            let pascal = p.struct_name.trim_end_matches("Plugin");
            let _ = writeln!(buf, "  load{pascal}I18n,");
        }
        buf.push_str("];\n");
    }
    buf
}

fn rpc_barrel_path(plugin_name: &str) -> PathBuf {
    PathBuf::from(format!(
        "packages/generated/src/plugins/{plugin_name}/rpc.ts"
    ))
}

/// Whether a plugin ships any `.proto` files (and therefore has an RPC surface
/// and a generated barrel). UI-only plugins have none.
fn plugin_has_proto(source_root: &Path, plugin_name: &str) -> bool {
    let mut files = Vec::new();
    collect_proto_files(&plugin_proto_dir(source_root, plugin_name), &mut files);
    !files.is_empty()
}

/// Write each RPC-bearing plugin's barrel under
/// `packages/generated/src/plugins/<name>/rpc.ts`; UI-only plugins (no `proto/`)
/// have no generated `*_pb.ts` to re-export, so they are skipped.
fn apply_rpc_barrels(
    plugins: &[ResolvedPlugin],
    source_root: &Path,
    dry_run: bool,
    changes: &mut Vec<FileChange>,
) -> Result<(), i32> {
    for plugin in plugins {
        if !plugin_has_proto(source_root, &plugin.name) {
            continue;
        }
        let rel = rpc_barrel_path(&plugin.name);
        let kind = apply_file(
            &source_root.join(&rel),
            &render_rpc_barrel(plugin, source_root),
            dry_run,
        )?;
        changes.push(FileChange {
            path: rel.display().to_string(),
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
    source_root: &Path,
    dry_run: bool,
    changes: &mut Vec<FileChange>,
) -> Result<(), i32> {
    for plugin in plugins {
        let Some(content) = render_consumer_registry_ts(plugin, plugins) else {
            continue;
        };
        let rel = consumer_registry_path(&plugin.name);
        let kind = apply_file(&source_root.join(&rel), &content, dry_run)?;
        changes.push(FileChange {
            path: rel.display().to_string(),
            kind,
        });
    }
    Ok(())
}

/// One service declared in a plugin's protos: its simple name (`HelloService`),
/// the generated `*_pb.js` module path (relative to a barrel), and its RPC
/// method names (camelCase, as protoc-gen-es emits them under `.method`).
struct ProtoServiceRef {
    name: String,
    pb_module: String,
    methods: Vec<String>,
}

/// The generated `*_pb.js` module path for a proto file, relative to a barrel at
/// `packages/generated/src/plugins/<x>/rpc.ts`. `buf generate` strips the module
/// root (`plugins/<plugin>/proto`), so `.../proto/hello/v1/hello.proto` becomes
/// `../../proto/hello/v1/hello_pb.js`.
fn pb_module_path(source_root: &Path, plugin_name: &str, proto_file: &Path) -> Option<String> {
    let root = plugin_proto_dir(source_root, plugin_name);
    let rel = proto_file.strip_prefix(&root).ok()?.to_str()?;
    let stem = rel.strip_suffix(".proto")?;
    Some(format!("../../proto/{stem}_pb.js"))
}

/// Lower-case the first character (`PascalCase` RPC name → protoc-gen-es method
/// key: `Greet` → `greet`, `ListGreetings` → `listGreetings`).
fn lower_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// All services (with methods + their pb module) declared by a plugin's protos.
fn plugin_services(plugin_name: &str, source_root: &Path) -> Vec<ProtoServiceRef> {
    let mut files = Vec::new();
    collect_proto_files(&plugin_proto_dir(source_root, plugin_name), &mut files);
    files.sort();
    let mut out = Vec::new();
    for file in files {
        let Some(pb_module) = pb_module_path(source_root, plugin_name, &file) else {
            continue;
        };
        if let Ok(content) = std::fs::read_to_string(&file) {
            for (name, methods) in scan_proto_service_methods(&content) {
                out.push(ProtoServiceRef {
                    name,
                    pb_module: pb_module.clone(),
                    methods: methods.iter().map(|m| lower_first(m)).collect(),
                });
            }
        }
    }
    out
}

/// Render a plugin's **narrow** RPC barrel: a `rpc` object exposing only the
/// plugin's own service methods plus the cross-plugin methods it declares in
/// `[dependencies.<dep>].rpc_methods`. An undeclared method isn't a key, so using
/// it is a TS error. Built from protoc-gen-es `GenService.method.<name>`
/// descriptors so `useQuery(rpc.Service.method, …)` still works.
fn render_rpc_barrel(plugin: &ResolvedPlugin, source_root: &Path) -> String {
    use std::collections::BTreeMap;
    // service name -> (pb module, sorted method set)
    let mut groups: BTreeMap<String, (String, std::collections::BTreeSet<String>)> =
        BTreeMap::new();

    // The plugin's own services expose every method.
    for svc in plugin_services(&plugin.name, source_root) {
        let entry = groups
            .entry(svc.name)
            .or_insert_with(|| (svc.pb_module.clone(), std::collections::BTreeSet::new()));
        entry.0 = svc.pb_module;
        entry.1.extend(svc.methods);
    }

    // Cross-plugin: only the methods named in [dependencies.<dep>].rpc_methods
    // (each `"ServiceName.MethodName"`), resolved against the dep's protos.
    for (dep_name, dep) in &plugin.manifest.dependencies {
        if dep.rpc_methods.is_empty() {
            continue;
        }
        let dep_services = plugin_services(dep_name, source_root);
        for spec in &dep.rpc_methods {
            let Some((service, method)) = spec.split_once('.') else {
                continue;
            };
            if let Some(found) = dep_services.iter().find(|s| s.name == service) {
                let entry = groups.entry(service.to_string()).or_insert_with(|| {
                    (found.pb_module.clone(), std::collections::BTreeSet::new())
                });
                entry.0.clone_from(&found.pb_module);
                entry.1.insert(lower_first(method));
            }
        }
    }

    // Imports, grouped by pb module (one import per module).
    let mut by_module: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
    for (service, (module, _)) in &groups {
        by_module
            .entry(module.clone())
            .or_default()
            .insert(service.clone());
    }

    let mut buf = String::with_capacity(512);
    buf.push_str(GENERATED_TS_HEADER);
    buf.push_str(
        "//\n\
         // Narrow RPC namespace: this plugin's own service methods plus the\n\
         // cross-plugin methods declared in [dependencies.<dep>].rpc_methods.\n\
         // An undeclared method is not a key here → a type error on use.\n\n",
    );
    for (module, services) in &by_module {
        let names = services.iter().cloned().collect::<Vec<_>>().join(", ");
        let _ = writeln!(buf, "import {{ {names} }} from '{module}';");
    }
    buf.push_str("\nexport const rpc = {\n");
    for (service, (_, methods)) in &groups {
        let _ = writeln!(buf, "  {service}: {{");
        for method in methods {
            let _ = writeln!(buf, "    {method}: {service}.method.{method},");
        }
        buf.push_str("  },\n");
    }
    buf.push_str("};\n");
    buf
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

/// Render a consumer plugin's typed `useComponent` wrapper: a local
/// `ComponentRegistry` interface mapping `'<dep>.<Component>'` → that component's
/// type (one entry per component a declared `[dependencies.<dep>]` exposes) plus
/// a keyed `useComponent` that wraps the SDK's untyped lookup. A typo or an
/// undeclared cross-plugin component is a compile error; the return type is the
/// exact component type. Returns `None` when no dependency exposes a component.
///
/// Each component's props type (`FooProps` for component `Foo`) is imported
/// type-only from the component's own module subpath (`@junius/plugin-<dep>/lib/Foo`,
/// from the manifest `module`) rather than the package index — the index's route
/// graph imports `@junius/sdk`, and pulling that in here is both unnecessary and a
/// source of resolution cycles. A plain interface (no `declare module`) keeps this
/// deterministic regardless of import order.
fn render_consumer_registry_ts(
    consumer: &ResolvedPlugin,
    plugins: &[ResolvedPlugin],
) -> Option<String> {
    let by_name: std::collections::HashMap<&str, &ResolvedPlugin> =
        plugins.iter().map(|p| (p.name.as_str(), p)).collect();

    // (plugin, component, module-subpath, props-import alias).
    let mut keys: Vec<(String, String, String, String)> = Vec::new();
    for dep_name in consumer.manifest.dependencies.keys() {
        if let Some(dep) = by_name.get(dep_name.as_str()) {
            for (component, exposed) in &dep.manifest.exposes.components {
                let subpath = exposed
                    .module
                    .strip_prefix("./")
                    .unwrap_or(&exposed.module)
                    .to_string();
                let alias = format!("{}_{}Props", to_pascal_case(&dep.name), component);
                keys.push((dep.name.clone(), component.clone(), subpath, alias));
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
         // Typed `useComponent` for this plugin: keyed to the components its\n\
         // declared dependencies expose. Import it here (not from `@junius/sdk`)\n\
         // to get key-checking + precise prop types; it wraps the SDK's untyped\n\
         // lookup. Each component `Foo` must export its `FooProps`.\n\n",
    );
    buf.push_str("import type { ComponentType } from 'react';\n");
    buf.push_str("import { useComponent as useComponentRaw } from '@junius/sdk';\n");
    for (plugin, component, subpath, alias) in &keys {
        let _ = writeln!(
            buf,
            "import type {{ {component}Props as {alias} }} from '@junius/plugin-{plugin}/{subpath}';",
        );
    }
    buf.push_str("\nexport interface ComponentRegistry {\n");
    for (plugin, component, _subpath, alias) in &keys {
        let _ = writeln!(buf, "  '{plugin}.{component}': ComponentType<{alias}>;");
    }
    buf.push_str("}\n\n");
    buf.push_str(
        "/** Look up a declared cross-plugin component, typed by its key. */\n\
         export function useComponent<K extends keyof ComponentRegistry>(\n  \
         key: K,\n\
         ): ComponentRegistry[K] | undefined {\n  \
         return useComponentRaw(key) as ComponentRegistry[K] | undefined;\n\
         }\n",
    );
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

/// One `- path: plugins/<name>/proto` line per enabled plugin that ships a
/// proto. M16 item A: removes the manual `buf.yaml` edit + `buf generate`
/// step the M13 friction log called out.
fn buf_yaml_module_lines(plugins: &[ResolvedPlugin], source_root: &Path) -> Vec<String> {
    plugins
        .iter()
        .filter(|p| plugin_has_proto(source_root, &p.name))
        .map(|p| format!("- path: plugins/{}/proto", p.name))
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

/// Reconcile the host frontend `package.json` `dependencies` with the enabled
/// plugins that ship a `frontend/` package. Adds `@junius/plugin-<name>:
/// "workspace:*"` for each (if missing), removes any `@junius/plugin-*` keys
/// no longer in the enabled set, and leaves every other dep alone. M16 item A:
/// closes the "no host-FE dep wiring" half of the M13 friction.
///
/// JSON has no comments, so we can't use the marker pattern. Instead we
/// rewrite only the `"dependencies": { … }` block in-place — top-level key
/// order, surrounding sections (`devDependencies`, `scripts`, etc.), and
/// trailing whitespace stay byte-identical. Existing `@junius/plugin-*` lines
/// are left alone if their value is already `"workspace:*"`; new entries are
/// inserted alphabetically among the other deps. Idempotent.
fn apply_host_frontend_deps(
    path: &Path,
    plugins: &[ResolvedPlugin],
    dry_run: bool,
) -> Result<&'static str, i32> {
    let existing = std::fs::read_to_string(path).map_err(|e| {
        eprintln!("junius: cannot read {}: {e}", path.display());
        exit::PARSE_ERROR
    })?;

    let new_content = match rewrite_deps_block(&existing, plugins) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("junius: cannot rewrite {}: {e}", path.display());
            return Err(exit::PARSE_ERROR);
        }
    };

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

/// Replace just the `"dependencies": { … }` block in a package.json. Validates
/// the file is valid JSON before/after, but never touches anything outside the
/// dependencies object — preserving the file's top-level key order, scripts,
/// devDependencies, etc. byte-for-byte.
fn rewrite_deps_block(src: &str, plugins: &[ResolvedPlugin]) -> Result<String, String> {
    // Parse to compute the desired dep map (preserves the existing non-plugin
    // entries — we read their key/values from the parsed JSON, then re-render
    // a fresh alphabetical block).
    let json: serde_json::Value = serde_json::from_str(src).map_err(|e| e.to_string())?;
    let deps = json
        .get("dependencies")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "no top-level `dependencies` object".to_string())?;

    let desired_plugin_keys: std::collections::BTreeSet<String> = plugins
        .iter()
        .filter(|p| p.has_frontend)
        .map(|p| format!("@junius/plugin-{}", p.name))
        .collect();

    // Final desired (key, value) set: every existing non-plugin dep + every
    // desired plugin dep with `workspace:*`. Plugin keys not in `desired` are
    // dropped (a disabled plugin).
    let mut final_entries: std::collections::BTreeMap<String, serde_json::Value> =
        std::collections::BTreeMap::new();
    for (k, v) in deps {
        if !k.starts_with("@junius/plugin-") {
            final_entries.insert(k.clone(), v.clone());
        }
    }
    for key in &desired_plugin_keys {
        final_entries.insert(key.clone(), serde_json::Value::String("workspace:*".into()));
    }

    // Render the block content (entries indented under "dependencies"). 2-space
    // indent + alphabetical keys, matching the file's existing convention.
    let indent = detect_indent(src);
    let mut block = String::new();
    block.push_str("{\n");
    let mut iter = final_entries.iter().peekable();
    while let Some((k, v)) = iter.next() {
        let value_str = serde_json::to_string(v).map_err(|e| e.to_string())?;
        let comma = if iter.peek().is_some() { "," } else { "" };
        let _ = writeln!(
            block,
            "{indent}{indent}{key}: {value}{comma}",
            indent = indent,
            key = serde_json::to_string(k).map_err(|e| e.to_string())?,
            value = value_str,
        );
    }
    let _ = write!(block, "{indent}}}");

    // Locate and replace the existing `"dependencies": {...}` span. The opening
    // brace is the first `{` after the key; we walk braces to find its match.
    let key_pos = src
        .find("\"dependencies\"")
        .ok_or_else(|| "could not find `\"dependencies\"` key".to_string())?;
    let open_pos = src[key_pos..]
        .find('{')
        .map(|o| key_pos + o)
        .ok_or_else(|| "no `{` after `\"dependencies\"`".to_string())?;
    let close_pos = match_brace(src, open_pos)
        .ok_or_else(|| "unbalanced braces in `dependencies` block".to_string())?;

    let mut out = String::with_capacity(src.len() + block.len());
    out.push_str(&src[..open_pos]);
    out.push_str(&block);
    out.push_str(&src[close_pos + 1..]);

    // Sanity: the rewritten string must still be valid JSON.
    serde_json::from_str::<serde_json::Value>(&out)
        .map_err(|e| format!("rewrite produced invalid JSON: {e}"))?;

    Ok(out)
}

/// Detect the file's leading indent (2 vs 4 spaces vs a tab). Reads the first
/// non-empty indented line. Defaults to two spaces.
fn detect_indent(src: &str) -> String {
    for line in src.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed == line {
            continue;
        }
        let leading = &line[..line.len() - trimmed.len()];
        return leading.to_string();
    }
    "  ".into()
}

/// Find the matching closing `}` for the `{` at `open_pos`. Tracks string
/// literals (with `\` escapes) so a `}` inside a value doesn't terminate the
/// block. Returns the byte index of the `}`, or `None` if unbalanced.
fn match_brace(src: &str, open_pos: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    if bytes.get(open_pos) != Some(&b'{') {
        return None;
    }
    let mut depth: i32 = 0;
    let mut i = open_pos;
    let mut in_str = false;
    while i < bytes.len() {
        let b = bytes[i];
        if in_str {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' => {
                in_str = true;
                i += 1;
            }
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

/// Run a managed subprocess from `cwd`. Streams stdout/stderr through to the
/// terminal so the user sees what's happening (`buf generate` and `pnpm
/// install` can take seconds). Returns `Err(exit::PARSE_ERROR)` on failure,
/// after printing a diagnostic.
fn run_subprocess(program: &str, args: &[&str], cwd: &Path, label: &str) -> Result<(), i32> {
    eprintln!("junius sync: running {label}…");
    let status = match std::process::Command::new(program)
        .args(args)
        .current_dir(cwd)
        .status()
    {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "junius sync: failed to spawn {program} {args:?}: {e}\n\
                 (managed file changed but the follow-up step did not run; run it manually)"
            );
            return Err(exit::PARSE_ERROR);
        }
    };
    if !status.success() {
        eprintln!("junius sync: {label} exited non-zero");
        return Err(exit::PARSE_ERROR);
    }
    Ok(())
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
            has_frontend: true,
            has_frontend_i18n: false,
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
        assert!(out.contains("import { useComponent as useComponentRaw } from '@junius/sdk';"));
        assert!(out.contains(
            "import type { GreeterCardProps as Hello_GreeterCardProps } from '@junius/plugin-hello/lib/GreeterCard';"
        ));
        assert!(out.contains("export interface ComponentRegistry {"));
        assert!(out.contains("'hello.GreeterCard': ComponentType<Hello_GreeterCardProps>;"));
        assert!(out.contains("'widgets.VenuePicker': ComponentType<Widgets_VenuePickerProps>;"));
        assert!(out.contains("export function useComponent<K extends keyof ComponentRegistry>("));

        // A plugin with no component-exposing deps gets no wrapper.
        assert!(render_consumer_registry_ts(&hello, &plugins).is_none());
    }

    // M16 item A regression: `rewrite_deps_block` swaps the
    // `"dependencies": { … }` span and leaves the surrounding JSON
    // byte-identical (top-level key order, devDependencies, scripts, etc.).
    #[test]
    fn rewrite_deps_block_inserts_plugin_dep_alphabetically() {
        let src = r#"{
  "name": "@junius/shell",
  "version": "0.0.0",
  "dependencies": {
    "@junius/design": "workspace:*",
    "react": "^19.0.0"
  },
  "devDependencies": {
    "vite": "^6.0.0"
  }
}
"#;
        let plugin = ResolvedPlugin {
            name: "events".into(),
            crate_name: "events-plugin".into(),
            module_name: "events_plugin".into(),
            struct_name: "EventsPlugin".into(),
            manifest: PluginManifest::parse(
                "[plugin]\nname = \"events\"\ndisplay_name = \"Events\"\nmanifest_schema = 1\n",
            )
            .unwrap(),
            has_frontend: true,
            has_frontend_i18n: false,
        };
        let out = rewrite_deps_block(src, std::slice::from_ref(&plugin)).unwrap();
        // Plugin dep added in the right alphabetical position.
        assert!(out.contains("\"@junius/design\": \"workspace:*\""));
        assert!(out.contains("\"@junius/plugin-events\": \"workspace:*\""));
        assert!(out.contains("\"react\": \"^19.0.0\""));
        // Surrounding sections still present, in order.
        assert!(
            out.find("\"name\"").unwrap() < out.find("\"dependencies\"").unwrap(),
            "top-level key order disturbed"
        );
        assert!(
            out.find("\"devDependencies\"").unwrap() > out.find("\"dependencies\"").unwrap(),
            "devDependencies moved"
        );
        // Idempotent: re-running produces the same string.
        assert_eq!(
            rewrite_deps_block(&out, std::slice::from_ref(&plugin)).unwrap(),
            out
        );
    }

    #[test]
    fn rewrite_deps_block_removes_disabled_plugin() {
        let src = r#"{
  "dependencies": {
    "@junius/plugin-old": "workspace:*",
    "react": "^19.0.0"
  }
}
"#;
        // No plugins → all `@junius/plugin-*` entries are dropped, non-plugin
        // deps stay.
        let out = rewrite_deps_block(src, &[]).unwrap();
        assert!(!out.contains("@junius/plugin-old"));
        assert!(out.contains("\"react\": \"^19.0.0\""));
    }

    #[test]
    fn detect_indent_returns_two_spaces_for_a_two_space_file() {
        let src = "{\n  \"a\": 1\n}\n";
        assert_eq!(detect_indent(src), "  ");
    }

    #[test]
    fn match_brace_handles_strings_with_braces() {
        let src = r#"{"key": "value with } inside"}"#;
        let close = match_brace(src, 0).unwrap();
        assert_eq!(close, src.len() - 1);
    }

    #[test]
    fn enumerate_all_plugins_walks_plugins_dir_sorted() {
        // The repo root has `plugins/{admin,events,...}/plugin.toml`. Walk
        // it from the workspace root (CARGO_MANIFEST_DIR is `tools/junius`).
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .unwrap();
        let names = enumerate_all_plugins(workspace_root).unwrap();
        // At least the two plugins the monorepo currently ships must show
        // up, sorted lexicographically.
        assert!(names.contains(&"admin".to_string()), "got {names:?}");
        assert!(names.contains(&"events".to_string()), "got {names:?}");
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(
            names, sorted,
            "enumerate_all_plugins should return sorted names"
        );
    }

    #[test]
    fn enumerate_all_plugins_ignores_non_plugin_dirs() {
        // A `plugins/` directory containing one valid plugin + one
        // unrelated subdir (no plugin.toml) should return only the
        // valid one.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let p_dir = root.join("plugins").join("widget");
        std::fs::create_dir_all(&p_dir).unwrap();
        std::fs::write(
            p_dir.join("plugin.toml"),
            "[plugin]\nname = \"widget\"\ndisplay_name = \"Widget\"\nmanifest_schema = 1\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("plugins").join("not-a-plugin")).unwrap();
        let names = enumerate_all_plugins(root).unwrap();
        assert_eq!(names, vec!["widget"]);
    }
}
