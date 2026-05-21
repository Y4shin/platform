//! Axum server: build the composed router, bind, serve with graceful shutdown.

use axum::{Router, routing::get};
use junius_sdk::Plugin;

use crate::boot;
use crate::config::HostConfig;

/// Compose the host's base routes with every plugin's contributed routes.
/// Each plugin is mounted under `metadata().mount.http_prefix`.
///
/// At M02 there are no plugins, so this returns just the root route.
pub fn build_app(plugins: &[Box<dyn Plugin>]) -> Router {
    let mut app = Router::new().route("/", get(|| async { "hello, platform" }));

    for plugin in plugins {
        let metadata = plugin.metadata();
        let resources = boot::build_resources_for(metadata.name);
        let router = plugin.routes(resources);
        app = app.nest(metadata.mount.http_prefix, router);
    }

    app
}

/// Boot the platform end-to-end:
/// 1. Run `on_startup` for every plugin.
/// 2. Bind `config.bind_addr` and start `axum::serve` with a Ctrl-C graceful
///    shutdown handler.
/// 3. After the server returns, run `on_shutdown` for every plugin in reverse.
pub async fn run(config: HostConfig, plugins: Vec<Box<dyn Plugin>>) -> anyhow::Result<()> {
    boot::run_startup(&plugins).await?;

    let app = build_app(&plugins);

    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    let local_addr = listener.local_addr()?;
    tracing::info!(addr = %local_addr, "juniusd listening");

    let shutdown = async {
        // Failure to install the Ctrl-C handler shouldn't abort the server —
        // the runtime still tears down cleanly when dropped.
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("juniusd shutting down");
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;

    boot::run_shutdown(&plugins).await;
    Ok(())
}
