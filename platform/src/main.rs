//! `juniusd` — the Junius host binary. Constructs the host config, initialises
//! telemetry (fmt logging + optional OTLP export), loads the generated plugin
//! registry, and hands off to `platform::server::run`.

use std::path::PathBuf;

use anyhow::Context as _;
use platform::config::HostConfig;
use platform::telemetry::init_telemetry;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Order: config → telemetry → plugins → run. Telemetry needs `[config.otel]`,
    // so config loads first; the guard then outlives `run` so the OTLP export
    // pipelines flush on shutdown.
    let path = config_path();
    let config = HostConfig::load_from_toml(&path)
        .with_context(|| format!("loading host config from {}", path.display()))?;
    let otel = config
        .resolved
        .as_ref()
        .map(|r| r.otel.clone())
        .unwrap_or_default();
    let (_telemetry_guard, metric_sink) = init_telemetry(&otel)?;

    let plugins = platform::generated::plugins::plugins();
    platform::server::run(config, plugins, metric_sink).await
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
