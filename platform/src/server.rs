//! Axum server: compose the router, bind, serve with graceful shutdown.

use std::collections::BTreeMap;

use axum::routing::get;
use axum::{Extension, Router};
use junius_sdk::Plugin;
use sqlx::PgPool;

use crate::auth::{self, AuthState};
use crate::boot;
use crate::config::{HostConfig, PluginRuntime};
use crate::db::{DbBootstrap, PluginPools};

/// Compose the host base + plugin routes **without** database-backed services.
/// Plugin routes that extract `PluginResources` will 500 (no context attached);
/// use this for resource-less routes (and tests). The full host uses
/// [`build_app_with_services`].
pub fn build_app(plugins: &[Box<dyn Plugin>]) -> Router {
    let (http, rpc) = compose(plugins, None);
    base_app().merge(http).nest("/rpc", rpc)
}

/// Compose the full host: per-plugin resource context, the `/api/auth/*` public
/// endpoints, `/api/me`, and the session middleware.
pub fn build_app_with_services(
    plugins: &[Box<dyn Plugin>],
    pools: &PluginPools,
    platform_pool: &PgPool,
    runtimes: &BTreeMap<String, PluginRuntime>,
    auth_state: AuthState,
) -> Router {
    let (http, rpc) = compose(plugins, Some((pools, platform_pool, runtimes)));
    let protected = http
        .nest("/rpc", rpc)
        .route("/api/me", get(auth::me::handler))
        .layer(axum::middleware::from_fn_with_state(
            auth_state.clone(),
            auth::session::middleware,
        ));

    base_app()
        .merge(auth::public_router(auth_state))
        .merge(protected)
        .layer(tower_cookies::CookieManagerLayer::new())
}

/// Nest every plugin's HTTP + RPC routers, attaching the per-plugin
/// `PluginResourceCtx` as an `Extension` when `services` is provided.
fn compose(
    plugins: &[Box<dyn Plugin>],
    services: Option<(&PluginPools, &PgPool, &BTreeMap<String, PluginRuntime>)>,
) -> (Router, Router) {
    let mut http = Router::new();
    let mut rpc = Router::new();
    for plugin in plugins {
        let metadata = plugin.metadata();
        let mut plugin_http = plugin.routes();
        let mut plugin_rpc = plugin.rpc_routes();

        if let Some((pools, platform_pool, runtimes)) = services {
            if let Some(db) = pools.get(metadata.name) {
                let runtime = runtimes.get(metadata.name).cloned().unwrap_or_default();
                let ctx = boot::build_ctx(metadata.name, db, platform_pool, &runtime);
                plugin_http = plugin_http.layer(Extension(ctx.clone()));
                plugin_rpc = plugin_rpc.layer(Extension(ctx));
            } else {
                tracing::error!(plugin = metadata.name, "no DB pool; resources unavailable");
            }
        }

        http = http.nest(metadata.mount.http_prefix, plugin_http);
        rpc = rpc.merge(plugin_rpc);
    }
    (http, rpc)
}

#[cfg(not(feature = "embed-frontend"))]
fn base_app() -> Router {
    Router::new().route("/", axum::routing::get(|| async { "hello, platform" }))
}

#[cfg(feature = "embed-frontend")]
fn base_app() -> Router {
    crate::static_assets::router()
}

/// Boot the platform end-to-end: connect the DB, build per-plugin pools and auth
/// state, run `on_startup`, serve until Ctrl-C, then run `on_shutdown`.
pub async fn run(config: HostConfig, plugins: Vec<Box<dyn Plugin>>) -> anyhow::Result<()> {
    let resolved = config.resolved.as_ref().ok_or_else(|| {
        anyhow::anyhow!("no [config] loaded; juniusd needs --config or JUNIUS_CONFIG")
    })?;

    let db = DbBootstrap::connect(&resolved.database_url).await?;
    let platform_pool = db.platform_pool().clone();
    let pools = db
        .build_plugin_pools(
            &resolved.database_url,
            &resolved.role_password_secret,
            &plugins,
        )
        .await?;

    boot::run_startup(&plugins, &pools, &platform_pool, &config.plugins).await?;

    // Browser-facing OIDC callback. When `oidc_redirect_url` is configured (e.g.
    // the Vite `:5173` origin under `junius dev`, so the callback flows through
    // the single dev origin), use it verbatim; otherwise derive it from the
    // bind address. This URL must also be registered with the IdP.
    let redirect_uri = resolved.oidc_redirect_url.clone().unwrap_or_else(|| {
        format!(
            "http://localhost:{}/api/auth/callback",
            config.bind_addr.port()
        )
    });
    let auth_state =
        AuthState::from_config(platform_pool.clone(), resolved, &redirect_uri, false).await;

    let app = build_app_with_services(
        &plugins,
        &pools,
        &platform_pool,
        &config.plugins,
        auth_state,
    );

    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    let local_addr = listener.local_addr()?;
    tracing::info!(addr = %local_addr, "juniusd listening");

    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("juniusd shutting down");
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;

    boot::run_shutdown(&plugins, &pools, &platform_pool, &config.plugins).await;
    Ok(())
}
