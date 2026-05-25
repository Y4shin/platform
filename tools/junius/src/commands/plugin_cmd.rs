//! `junius plugin {list,info,enable,disable}`.

use std::path::{Path, PathBuf};

use junius_manifest::{PlatformManifest, PluginManifest};
use serde::Serialize;

use crate::cli::{OutputFormat, PluginCmd};
use crate::output::{self, Renderable};
use crate::{cache, exit, source};

pub fn run(cmd: &PluginCmd, format: OutputFormat) -> i32 {
    match cmd {
        PluginCmd::List { config } => list(config.as_deref(), format),
        PluginCmd::Info { name } => info(name, format),
        PluginCmd::Enable { name, config } => enable(name, config.as_deref(), format),
        PluginCmd::Disable { name, config } => disable(name, config.as_deref(), format),
    }
}

// --- list ---------------------------------------------------------------------

#[derive(Serialize)]
struct ListResult {
    config: String,
    enabled: Vec<String>,
}

impl Renderable for ListResult {
    fn render_plain(&self, w: &mut dyn std::io::Write) -> std::io::Result<()> {
        writeln!(w, "Enabled plugins (from {}):", self.config)?;
        if self.enabled.is_empty() {
            writeln!(w, "  (none)")?;
        } else {
            for name in &self.enabled {
                writeln!(w, "  - {name}")?;
            }
        }
        Ok(())
    }
}

fn list(config: Option<&Path>, format: OutputFormat) -> i32 {
    let path = config.map_or_else(|| PathBuf::from("platform.toml"), PathBuf::from);

    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("junius: cannot read {}: {e}", path.display());
            return exit::PARSE_ERROR;
        }
    };

    let manifest = match PlatformManifest::parse(&src) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("junius: parse error in {}: {e}", path.display());
            return exit::PARSE_ERROR;
        }
    };

    let result = ListResult {
        config: path.display().to_string(),
        enabled: manifest.plugins.enabled,
    };
    let _ = output::emit(format, &result);
    exit::OK
}

// --- info ---------------------------------------------------------------------

#[derive(Serialize)]
struct InfoResult {
    name: String,
    display_name: String,
    description: Option<String>,
    mount: MountJson,
    dependencies: Vec<DepJson>,
    exposed_components: Vec<String>,
    exposed_tables: Vec<String>,
    permissions: Vec<String>,
    capabilities: Vec<String>,
}

#[derive(Serialize)]
#[allow(clippy::struct_field_names)]
struct MountJson {
    route_prefix: String,
    rpc_prefix: String,
    http_prefix: String,
}

#[derive(Serialize)]
struct DepJson {
    name: String,
    optional: bool,
}

impl Renderable for InfoResult {
    fn render_plain(&self, w: &mut dyn std::io::Write) -> std::io::Result<()> {
        writeln!(w, "Plugin: {} ({})", self.name, self.display_name)?;
        if let Some(desc) = &self.description {
            writeln!(w, "  description: {desc}")?;
        }
        writeln!(w, "  mount:")?;
        writeln!(w, "    route_prefix = {}", self.mount.route_prefix)?;
        writeln!(w, "    rpc_prefix   = {}", self.mount.rpc_prefix)?;
        writeln!(w, "    http_prefix  = {}", self.mount.http_prefix)?;

        writeln!(w, "  dependencies:")?;
        if self.dependencies.is_empty() {
            writeln!(w, "    (none)")?;
        } else {
            for d in &self.dependencies {
                let kind = if d.optional { "optional" } else { "required" };
                writeln!(w, "    - {} ({kind})", d.name)?;
            }
        }

        writeln!(
            w,
            "  exposed components: {}",
            join_or_none(&self.exposed_components)
        )?;
        writeln!(
            w,
            "  exposed tables:     {}",
            join_or_none(&self.exposed_tables)
        )?;
        writeln!(
            w,
            "  permissions:        {}",
            join_or_none(&self.permissions)
        )?;
        writeln!(
            w,
            "  capabilities:       {}",
            join_or_none(&self.capabilities)
        )?;
        Ok(())
    }
}

fn join_or_none(xs: &[String]) -> String {
    if xs.is_empty() {
        "(none)".into()
    } else {
        xs.join(", ")
    }
}

fn info(name: &str, format: OutputFormat) -> i32 {
    let path: PathBuf = ["plugins", name, "plugin.toml"].iter().collect();
    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("junius: cannot read {}: {e}", path.display());
            return exit::PARSE_ERROR;
        }
    };

    let manifest = match PluginManifest::parse(&src) {
        Ok(m) => m.with_defaults(),
        Err(e) => {
            eprintln!("junius: parse error in {}: {e}", path.display());
            return exit::PARSE_ERROR;
        }
    };

    let issues = manifest.validate();
    if !issues.is_ok() {
        for i in issues.errors() {
            eprintln!("[error] {} at {}: {}", i.code, i.path, i.message);
        }
        return exit::VALIDATION;
    }

    let result = build_info(&manifest);
    let _ = output::emit(format, &result);
    exit::OK
}

// --- enable / disable ---------------------------------------------------------

/// Resolve the source for a deployment + load every enabled plugin's manifest.
/// Returns `(config_path, source_root, manifest, plugin_manifests)`.
fn load_deployment(
    config: Option<&Path>,
) -> Result<
    (
        PathBuf,
        PathBuf,
        PlatformManifest,
        std::collections::BTreeMap<String, PluginManifest>,
    ),
    i32,
> {
    let config_path =
        absolutize(config.map_or_else(|| PathBuf::from("platform.toml"), PathBuf::from));
    let deployment_dir = config_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let src = std::fs::read_to_string(&config_path).map_err(|e| {
        eprintln!("junius: cannot read {}: {e}", config_path.display());
        exit::PARSE_ERROR
    })?;
    let manifest = PlatformManifest::parse(&src).map_err(|e| {
        eprintln!("junius: parse error in {}: {e}", config_path.display());
        exit::PARSE_ERROR
    })?;
    let resolved = source::resolve(
        &manifest.source,
        &deployment_dir,
        &cache::sources_dir(),
        &source::ResolveOpts::default(),
    )
    .map_err(|e| {
        eprintln!("junius: resolving [source]: {e:#}");
        exit::VALIDATION
    })?;
    let mut plugins = std::collections::BTreeMap::new();
    for name in &manifest.plugins.enabled {
        if let Some(m) = read_plugin(&resolved.root, name) {
            plugins.insert(name.clone(), m);
        }
    }
    Ok((config_path, resolved.root, manifest, plugins))
}

fn read_plugin(source_root: &Path, name: &str) -> Option<PluginManifest> {
    let path = source_root.join("plugins").join(name).join("plugin.toml");
    let src = std::fs::read_to_string(path).ok()?;
    PluginManifest::parse(&src).ok()
}

fn enable(name: &str, config: Option<&Path>, format: OutputFormat) -> i32 {
    let (config_path, source_root, manifest, _plugins) = match load_deployment(config) {
        Ok(v) => v,
        Err(code) => return code,
    };
    if manifest.plugins.enabled.iter().any(|n| n == name) {
        println!("junius: plugin {name:?} is already enabled");
        return exit::OK;
    }
    // The plugin must exist in the source.
    let Some(plugin) = read_plugin(&source_root, name) else {
        eprintln!(
            "junius: plugin {name:?} not found at {}/plugins/{name}/plugin.toml",
            source_root.display()
        );
        return exit::VALIDATION;
    };
    // Its required (non-optional) dependencies must already be enabled.
    let missing: Vec<&String> = plugin
        .dependencies
        .iter()
        .filter(|(_, dep)| !dep.optional)
        .map(|(dep_name, _)| dep_name)
        .filter(|dep_name| !manifest.plugins.enabled.iter().any(|n| &n == dep_name))
        .collect();
    if !missing.is_empty() {
        eprintln!(
            "junius: cannot enable {name:?}: required dependencies not enabled: {}",
            missing
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        return exit::VALIDATION;
    }
    if let Err(code) = edit_enabled(&config_path, |arr| {
        arr.push(name);
    }) {
        return code;
    }
    println!("junius: enabled {name:?}");
    super::sync::run_in(&source_root, &config_path, false, format)
}

fn disable(name: &str, config: Option<&Path>, format: OutputFormat) -> i32 {
    let (config_path, source_root, manifest, plugins) = match load_deployment(config) {
        Ok(v) => v,
        Err(code) => return code,
    };
    if !manifest.plugins.enabled.iter().any(|n| n == name) {
        println!("junius: plugin {name:?} is not enabled");
        return exit::OK;
    }
    // Refuse if another enabled plugin requires it.
    let dependents: Vec<&String> = plugins
        .iter()
        .filter(|(other, _)| other.as_str() != name)
        .filter(|(_, m)| m.dependencies.get(name).is_some_and(|dep| !dep.optional))
        .map(|(other, _)| other)
        .collect();
    if !dependents.is_empty() {
        eprintln!(
            "junius: cannot disable {name:?}: required by {}",
            dependents
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        return exit::VALIDATION;
    }
    if let Err(code) = edit_enabled(&config_path, |arr| {
        arr.retain(|v| v.as_str() != Some(name));
    }) {
        return code;
    }
    println!("junius: disabled {name:?}");
    super::sync::run_in(&source_root, &config_path, false, format)
}

/// Mutate `[plugins].enabled` in `config_path` in place, preserving comments +
/// formatting via `toml_edit`.
fn edit_enabled(config_path: &Path, mutate: impl FnOnce(&mut toml_edit::Array)) -> Result<(), i32> {
    let src = std::fs::read_to_string(config_path).map_err(|e| {
        eprintln!("junius: cannot read {}: {e}", config_path.display());
        exit::PARSE_ERROR
    })?;
    let mut doc = src.parse::<toml_edit::DocumentMut>().map_err(|e| {
        eprintln!("junius: parse error in {}: {e}", config_path.display());
        exit::PARSE_ERROR
    })?;
    let Some(arr) = doc
        .get_mut("plugins")
        .and_then(|p| p.get_mut("enabled"))
        .and_then(toml_edit::Item::as_array_mut)
    else {
        eprintln!(
            "junius: {} has no [plugins].enabled array",
            config_path.display()
        );
        return Err(exit::VALIDATION);
    };
    mutate(arr);
    std::fs::write(config_path, doc.to_string()).map_err(|e| {
        eprintln!("junius: writing {}: {e}", config_path.display());
        exit::PARSE_ERROR
    })
}

fn absolutize(p: PathBuf) -> PathBuf {
    if p.is_absolute() {
        p
    } else if let Ok(cwd) = std::env::current_dir() {
        cwd.join(p)
    } else {
        p
    }
}

fn build_info(m: &PluginManifest) -> InfoResult {
    InfoResult {
        name: m.plugin.name.clone(),
        display_name: m.plugin.display_name.clone(),
        description: m.plugin.description.clone(),
        mount: MountJson {
            route_prefix: m.mount.route_prefix.clone().unwrap_or_default(),
            rpc_prefix: m.mount.rpc_prefix.clone().unwrap_or_default(),
            http_prefix: m.mount.http_prefix.clone().unwrap_or_default(),
        },
        dependencies: m
            .dependencies
            .iter()
            .map(|(k, v)| DepJson {
                name: k.clone(),
                optional: v.optional,
            })
            .collect(),
        exposed_components: m.exposes.components.keys().cloned().collect(),
        exposed_tables: m.exposes.tables.keys().cloned().collect(),
        permissions: m.permissions.keys().cloned().collect(),
        capabilities: m.requires.capabilities.clone(),
    }
}
