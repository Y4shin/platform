//! `juniusd` — the Junius host binary. Constructs the host config, loads the
//! generated plugin registry, initialises tracing, and hands off to
//! `platform::server::run`.

mod generated;

use std::path::PathBuf;

use anyhow::Context as _;
use platform::config::HostConfig;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let path = config_path();
    let config = HostConfig::load_from_toml(&path)
        .with_context(|| format!("loading host config from {}", path.display()))?;
    let plugins = generated::plugins::plugins();
    platform::server::run(config, plugins).await
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

fn init_tracing() {
    // `RUST_LOG` is the standard tracing developer-debug knob (see
    // `platform::config` for the policy on env reads).
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}
