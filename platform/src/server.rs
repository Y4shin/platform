//! Axum server: build the composed router, bind, serve with graceful shutdown.

use axum::Router;
use junius_sdk::Plugin;

use crate::boot;
use crate::config::HostConfig;

/// Compose the host's base routes with every plugin's contributed routes.
/// Each plugin is mounted under `metadata().mount.http_prefix`.
///
/// With the `embed-frontend` feature on, the SPA bundle is served as a
/// fallback so client-side `TanStack Router` routes (`/p/<plugin>/...`)
/// resolve to `index.html`. In dev mode the fallback is a simple text
/// banner — Vite serves the SPA on a different port.
pub fn build_app(plugins: &[Box<dyn Plugin>]) -> Router {
    let mut app = base_app();
    let mut rpc_root = Router::new();

    for plugin in plugins {
        let metadata = plugin.metadata();
        let resources = boot::build_resources_for(metadata.name);

        app = app.nest(metadata.mount.http_prefix, plugin.routes(resources.clone()));
        rpc_root = rpc_root.merge(plugin.rpc_routes(resources));
    }

    app.nest("/rpc", rpc_root)
}

#[cfg(not(feature = "embed-frontend"))]
fn base_app() -> Router {
    Router::new().route("/", axum::routing::get(|| async { "hello, platform" }))
}

#[cfg(feature = "embed-frontend")]
fn base_app() -> Router {
    crate::static_assets::router()
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
