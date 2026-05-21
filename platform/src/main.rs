//! `juniusd` — the Junius host binary. Constructs the host config, loads the
//! generated plugin registry, initialises tracing, and hands off to
//! `platform::server::run`.

mod generated;

use platform::config::HostConfig;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let config = HostConfig::default();
    let plugins = generated::plugins::plugins();
    platform::server::run(config, plugins).await
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
