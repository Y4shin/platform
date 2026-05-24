//! `junius migrate` — apply host + plugin migrations and emit role grants.
//!
//! `up`/`status` load the deployment config, resolve `[config]` secrets, and
//! drive the runner in [`crate::migrate`]. `down` is intentionally unsupported:
//! v1 uses forward-fix migrations.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use junius_manifest::{PlatformManifest, PluginManifest, ResolvedConfig, secrets};

use junius::migrate;

use crate::cli::MigrateCmd;
use crate::exit;

const DEFAULT_CONFIG: &str = "platform.toml";

pub fn run(cmd: &MigrateCmd) -> i32 {
    match cmd {
        MigrateCmd::Up { config } => run_up(config.clone()),
        MigrateCmd::Status { config } => run_status(config.clone()),
        MigrateCmd::Down => {
            eprintln!(
                "junius: migrate down is not supported; v1 uses forward-fix migrations \
                 (write a new migration that reverses the change)"
            );
            exit::NOT_IMPLEMENTED
        }
    }
}

fn run_up(config: Option<PathBuf>) -> i32 {
    let Loaded {
        resolved,
        manifests,
        enabled,
    } = match load(config) {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("junius: cannot start async runtime: {e}");
            return exit::PARSE_ERROR;
        }
    };
    let result = rt.block_on(migrate::up(Path::new("."), &resolved, &manifests, &enabled));
    match result {
        Ok(report) => {
            if report.applied.is_empty() {
                println!("No pending migrations.");
            } else {
                println!(
                    "Applied {} migration(s) ({} already up to date):",
                    report.applied.len(),
                    report.already_applied
                );
                for label in &report.applied {
                    println!("  applied  {label}");
                }
            }
            if !report.roles_granted.is_empty() {
                println!("Granted roles: {}", report.roles_granted.join(", "));
            }
            exit::OK
        }
        Err(e) => {
            eprintln!("junius: migrate up failed: {e}");
            exit::PARSE_ERROR
        }
    }
}

fn run_status(config: Option<PathBuf>) -> i32 {
    let Loaded {
        resolved,
        manifests: _,
        enabled,
    } = match load(config) {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("junius: cannot start async runtime: {e}");
            return exit::PARSE_ERROR;
        }
    };
    let result = rt.block_on(migrate::status(Path::new("."), &resolved, &enabled));
    match result {
        Ok(report) => {
            println!("Applied ({}):", report.applied.len());
            for label in &report.applied {
                println!("  {label}");
            }
            println!("Pending ({}):", report.pending.len());
            for label in &report.pending {
                println!("  {label}");
            }
            exit::OK
        }
        Err(e) => {
            eprintln!("junius: migrate status failed: {e}");
            exit::PARSE_ERROR
        }
    }
}

struct Loaded {
    resolved: ResolvedConfig,
    manifests: BTreeMap<String, PluginManifest>,
    enabled: Vec<String>,
}

fn load(config: Option<PathBuf>) -> Result<Loaded, i32> {
    let config_path = config.unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG));

    let src = std::fs::read_to_string(&config_path).map_err(|e| {
        eprintln!("junius: cannot read {}: {e}", config_path.display());
        exit::PARSE_ERROR
    })?;
    let platform = PlatformManifest::parse(&src).map_err(|e| {
        eprintln!("junius: parse error in {}: {e}", config_path.display());
        exit::PARSE_ERROR
    })?;
    let report = platform.validate();
    if !report.is_ok() {
        for issue in report.errors() {
            eprintln!(
                "junius: [error] {} at {}: {}",
                issue.code, issue.path, issue.message
            );
        }
        return Err(exit::VALIDATION);
    }

    let resolved = secrets::resolve_config(&platform.config, &env_lookup()).map_err(|e| {
        eprintln!("junius: config error in {}: {e}", config_path.display());
        exit::PARSE_ERROR
    })?;

    let mut manifests = BTreeMap::new();
    for name in &platform.plugins.enabled {
        let plugin_toml = PathBuf::from("plugins").join(name).join("plugin.toml");
        let plugin_src = std::fs::read_to_string(&plugin_toml).map_err(|e| {
            eprintln!("junius: cannot read {}: {e}", plugin_toml.display());
            exit::PARSE_ERROR
        })?;
        let manifest = PluginManifest::parse(&plugin_src).map_err(|e| {
            eprintln!("junius: parse error in {}: {e}", plugin_toml.display());
            exit::PARSE_ERROR
        })?;
        manifests.insert(name.clone(), manifest);
    }

    Ok(Loaded {
        resolved,
        manifests,
        enabled: platform.plugins.enabled,
    })
}

/// Lookup closure for `env:` secret indirections. This is the single sanctioned
/// direct environment read in the CLI.
fn env_lookup() -> impl Fn(&str) -> Option<String> {
    #[allow(
        clippy::disallowed_methods,
        reason = "resolving env: secret indirections from platform.toml [config]"
    )]
    |var: &str| std::env::var(var).ok()
}
