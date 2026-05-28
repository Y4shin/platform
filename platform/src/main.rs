//! `juniusd` — the Junius host binary. Constructs the host config, initialises
//! telemetry (fmt logging + optional OTLP export), loads the generated plugin
//! registry, and hands off to `platform::server::run`.

use std::path::PathBuf;
use std::str::FromStr as _;

use anyhow::Context as _;
use junius_manifest::PlatformMode;
use platform::config::HostConfig;
use platform::telemetry::init_telemetry;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Order: config → telemetry → plugins → run. Telemetry needs `[config.otel]`,
    // so config loads first; the guard then outlives `run` so the OTLP export
    // pipelines flush on shutdown.
    let path = config_path();

    // `--check-config`: parse + resolve the deployment config (incl. `env:`/
    // `file:` secrets) and exit, without binding a socket or touching the
    // database. The deployment-build CI smoke test runs this.
    if check_config_requested() {
        HostConfig::load_from_toml(&path)
            .with_context(|| format!("loading host config from {}", path.display()))?;
        println!("juniusd: config OK ({})", path.display());
        return Ok(());
    }

    let mut config = HostConfig::load_from_toml(&path)
        .with_context(|| format!("loading host config from {}", path.display()))?;
    // M24: `JUNIUS_MODE` env var overrides `[build] mode` from the toml.
    // The image entrypoint sets it so a precompiled image is authoritative
    // about its own mode regardless of what the operator's toml says.
    if let Some(mode) = mode_from_env()? {
        config.build_mode = mode;
    }
    let otel = config
        .resolved
        .as_ref()
        .map(|r| r.otel.clone())
        .unwrap_or_default();
    let (_telemetry_guard, metric_sink) = init_telemetry(&otel)?;

    let plugins = platform::generated::plugins::plugins();
    platform::server::run(config, plugins, metric_sink).await
}

/// Read `JUNIUS_MODE`. Returns `Ok(None)` when unset, `Ok(Some(mode))`
/// when valid, or `Err` on a malformed value (no silent fallback).
fn mode_from_env() -> anyhow::Result<Option<PlatformMode>> {
    #[allow(
        clippy::disallowed_methods,
        reason = "JUNIUS_MODE is the M24 image-mode override env var"
    )]
    let Ok(raw) = std::env::var("JUNIUS_MODE") else {
        return Ok(None);
    };
    PlatformMode::from_str(&raw)
        .map(Some)
        .map_err(|e| anyhow::anyhow!("JUNIUS_MODE: {e}"))
}

/// Resolve the deployment config path: `--config <path>`, else `$JUNIUS_CONFIG`,
/// else `./platform.toml`.
fn config_path() -> PathBuf {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--config" {
            if let Some(path) = args.next() {
                return PathBuf::from(path);
            }
        } else if let Some(path) = arg.strip_prefix("--config=") {
            return PathBuf::from(path);
        }
    }
    #[allow(
        clippy::disallowed_methods,
        reason = "JUNIUS_CONFIG selects the deployment config file path"
    )]
    if let Ok(path) = std::env::var("JUNIUS_CONFIG") {
        return PathBuf::from(path);
    }
    PathBuf::from("platform.toml")
}

/// Whether `--check-config` was passed.
fn check_config_requested() -> bool {
    std::env::args().skip(1).any(|a| a == "--check-config")
}
