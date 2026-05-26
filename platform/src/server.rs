//! Axum server: compose the router, bind, serve with graceful shutdown.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use axum::routing::{get, post};
use axum::{Extension, Router};
use junius_sdk::{Locale, Localizer, MetricSink, Plugin, PluginResourceCtx};
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;

use crate::auth::{self, AuthState};
use crate::boot;
use crate::config::{HostConfig, PluginRuntime};
use crate::db::{DbBootstrap, PluginPools};
use crate::infra::HostInfra;

/// Compose the host base + plugin routes **without** database-backed services.
/// Plugin routes/RPC that need `PluginResources` will 500 (no context attached);
/// use this for resource-less routes (and tests). The full host uses
/// [`build_app_with_services`].
pub fn build_app(plugins: &[Box<dyn Plugin>]) -> Router {
    let http = compose_http(plugins, None);
    base_app().merge(http).nest("/rpc", build_rpc(plugins))
}

/// Compose the full host: per-plugin resource context, the `/api/auth/*` public
/// endpoints, `/api/me`, and the session middleware.
pub fn build_app_with_services(
    plugins: &[Box<dyn Plugin>],
    pools: &PluginPools,
    platform_pool: &PgPool,
    runtimes: &BTreeMap<String, PluginRuntime>,
    auth_state: AuthState,
    infra: &HostInfra,
    localizer: &Localizer,
) -> Router {
    let http = compose_http(
        plugins,
        Some((pools, platform_pool, runtimes, infra, localizer)),
    );

    // All plugins' Connect services are folded into one router under `/rpc`. Two
    // layers run before dispatch (inside the session middleware, so
    // `Extension<User>` is set): `inject_ctx` attaches the right plugin's
    // `PluginResourceCtx` (by service FQN) for the handler, and the permission
    // guard rejects unauthorized calls.
    let ctx_map = Arc::new(build_ctx_map(
        plugins,
        pools,
        platform_pool,
        runtimes,
        infra,
        localizer,
    ));
    let rpc = build_rpc(plugins)
        .layer(axum::middleware::from_fn(
            crate::rpc_guard::require_permissions,
        ))
        .layer(axum::middleware::from_fn_with_state(
            ctx_map,
            crate::rpc_guard::inject_ctx,
        ));

    let protected = http
        .nest("/rpc", rpc)
        .route("/api/me", get(auth::me::handler))
        .route(
            "/api/me/locale",
            post(auth::me::update_locale).with_state(auth_state.clone()),
        )
        .layer(axum::middleware::from_fn_with_state(
            auth_state.clone(),
            auth::session::middleware,
        ));

    base_app()
        .merge(auth::public_router(auth_state))
        .merge(protected)
        .layer(tower_cookies::CookieManagerLayer::new())
}

/// Tuple of host services needed to assemble a plugin's request context.
/// Factored out so `compose_http`'s signature reads cleanly and clippy stops
/// flagging it as "very complex type"; the tuple is purely a transport for
/// the boot-time references.
type HttpServices<'a> = (
    &'a PluginPools,
    &'a PgPool,
    &'a BTreeMap<String, PluginRuntime>,
    &'a HostInfra,
    &'a Localizer,
);

/// Nest every plugin's HTTP routes under its `http_prefix`, attaching the
/// per-plugin `PluginResourceCtx` as an `Extension` when `services` is provided.
fn compose_http(plugins: &[Box<dyn Plugin>], services: Option<HttpServices<'_>>) -> Router {
    let mut http = Router::new();
    for plugin in plugins {
        let metadata = plugin.metadata();
        let mut plugin_http = plugin.routes();
        if let Some((pools, platform_pool, runtimes, infra, localizer)) = services {
            if let Some(db) = pools.get(metadata.name) {
                let runtime = runtimes.get(metadata.name).cloned().unwrap_or_default();
                let ctx = boot::build_ctx(metadata, db, platform_pool, &runtime, infra, localizer);
                plugin_http = plugin_http.layer(Extension(ctx));
            } else {
                tracing::error!(plugin = metadata.name, "no DB pool; resources unavailable");
            }
        }
        http = http.nest(metadata.mount.http_prefix, plugin_http);
    }
    http
}

/// Fold every plugin's Connect services into one `connectrpc` router and convert
/// it to an axum router (mounted under `/rpc` by the caller).
fn build_rpc(plugins: &[Box<dyn Plugin>]) -> Router {
    let mut router = connectrpc::Router::new();
    for plugin in plugins {
        router = plugin.register_rpc(router);
    }
    router.into_axum_router()
}

/// Build the per-plugin `PluginResourceCtx` map the RPC `inject_ctx` layer uses
/// to attach the right context per request (keyed by plugin name).
fn build_ctx_map(
    plugins: &[Box<dyn Plugin>],
    pools: &PluginPools,
    platform_pool: &PgPool,
    runtimes: &BTreeMap<String, PluginRuntime>,
    infra: &HostInfra,
    localizer: &Localizer,
) -> HashMap<String, PluginResourceCtx> {
    let mut map = HashMap::new();
    for plugin in plugins {
        let meta = plugin.metadata();
        if let Some(db) = pools.get(meta.name) {
            let runtime = runtimes.get(meta.name).cloned().unwrap_or_default();
            map.insert(
                meta.name.to_string(),
                boot::build_ctx(meta, db, platform_pool, &runtime, infra, localizer),
            );
        }
    }
    map
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
#[allow(
    clippy::too_many_lines,
    reason = "boot wiring is inherently linear: each step (db → pools → infra → localizer → startup → worker → router → serve → shutdown) is one short block; splitting it would just push the wiring through tuple-returning helpers"
)]
pub async fn run(
    config: HostConfig,
    plugins: Vec<Box<dyn Plugin>>,
    metric_sink: Option<Arc<dyn MetricSink>>,
) -> anyhow::Result<()> {
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

    // Host-global infra clients (job backend, object stores, email transport,
    // OTel metric sink) — built once, shared across plugins via `build_ctx`.
    let infra = HostInfra::build(resolved, metric_sink).await?;

    // i18n catalog: one Localizer built once from every plugin's register_i18n,
    // then cloned into each plugin's request context. Static `&[…]` data, so the
    // build itself is just slice writes.
    let default_locale = resolved
        .default_locale
        .as_deref()
        .and_then(Locale::from_code)
        .unwrap_or_default();
    let localizer = boot::build_localizer(&plugins, default_locale);

    boot::run_startup(
        &plugins,
        &pools,
        &platform_pool,
        &config.plugins,
        &infra,
        &localizer,
    )
    .await?;

    // Background job worker: consume each plugin's jobs with that plugin's own
    // (caller-less) resources. Cancelled first on shutdown so in-flight jobs drain
    // before the HTTP server and plugin `on_shutdown` hooks run.
    let worker_cancel = CancellationToken::new();
    let worker = spawn_worker(
        &plugins,
        &pools,
        &platform_pool,
        &config,
        &infra,
        &localizer,
        &worker_cancel,
    );

    // Host audit-retention prune (interval task; cancelled with the worker token).
    let audit_prune = crate::jobs::audit_prune::spawn(
        platform_pool.clone(),
        resolved.audit.retention_days,
        worker_cancel.clone(),
    );

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

    let mut app = build_app_with_services(
        &plugins,
        &pools,
        &platform_pool,
        &config.plugins,
        auth_state,
        &infra,
        &localizer,
    );

    // juniusd-mediated storage endpoints (token-authed, no session) — mounted
    // when a storage token secret is configured.
    if let Some(signer) = &infra.storage.token_signer {
        app = app.merge(crate::storage::http::router(
            crate::storage::http::StorageHttpState {
                stores: infra.storage.stores.clone(),
                signer: signer.clone(),
                platform_pool: platform_pool.clone(),
            },
        ));
    }

    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    let local_addr = listener.local_addr()?;
    tracing::info!(addr = %local_addr, "juniusd listening");

    let shutdown = {
        let worker_cancel = worker_cancel.clone();
        async move {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("juniusd shutting down");
            // Stop consuming + drain in-flight jobs before HTTP/lifecycle teardown.
            worker_cancel.cancel();
        }
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;

    // The worker was signalled in `shutdown`; await its drain before lifecycle hooks.
    if let Some(handle) = worker {
        if let Err(e) = handle.await {
            tracing::error!(error = %e, "job worker task panicked");
        }
    }
    audit_prune.abort();

    boot::run_shutdown(
        &plugins,
        &pools,
        &platform_pool,
        &config.plugins,
        &infra,
        &localizer,
    )
    .await;
    Ok(())
}

/// Build the job registry from the plugins and spawn the worker. Returns `None`
/// when no broker is configured or no plugin registers jobs.
fn spawn_worker(
    plugins: &[Box<dyn Plugin>],
    pools: &PluginPools,
    platform_pool: &PgPool,
    config: &HostConfig,
    infra: &HostInfra,
    localizer: &Localizer,
    cancel: &CancellationToken,
) -> Option<tokio::task::JoinHandle<()>> {
    let pool = infra.jobs.pool.clone()?;
    let mut registry = crate::jobs::worker::JobRegistry::new();
    for plugin in plugins {
        let meta = plugin.metadata();
        let handlers = plugin.jobs();
        if handlers.is_empty() {
            continue;
        }
        let Some(db) = pools.get(meta.name) else {
            tracing::error!(plugin = meta.name, "no DB pool; skipping job handlers");
            continue;
        };
        let runtime = config.plugins.get(meta.name).cloned().unwrap_or_default();
        let ctx = boot::build_ctx(meta, db, platform_pool, &runtime, infra, localizer);
        registry.register_plugin(handlers, &ctx);
    }
    if registry.is_empty() {
        return None;
    }

    let platform_pool = platform_pool.clone();
    let prefetch = config.resolved.as_ref().map_or(4, |r| r.job_workers);
    let cancel = cancel.clone();
    Some(tokio::spawn(async move {
        if let Err(e) =
            crate::jobs::worker::run_worker(pool, registry, platform_pool, prefetch, cancel).await
        {
            tracing::error!(error = %e, "job worker exited with error");
        }
    }))
}
