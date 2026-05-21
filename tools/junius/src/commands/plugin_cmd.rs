//! `junius plugin {list,info,enable,disable}`.

use std::path::{Path, PathBuf};

use junius_manifest::{PlatformManifest, PluginManifest};
use serde::Serialize;

use super::not_implemented;
use crate::cli::{OutputFormat, PluginCmd};
use crate::exit;
use crate::output::{self, Renderable};

pub fn run(cmd: &PluginCmd, format: OutputFormat) -> i32 {
    match cmd {
        PluginCmd::List { config } => list(config.as_deref(), format),
        PluginCmd::Info { name } => info(name, format),
        PluginCmd::Enable { .. } => not_implemented("plugin enable", "M11"),
        PluginCmd::Disable { .. } => not_implemented("plugin disable", "M11"),
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
