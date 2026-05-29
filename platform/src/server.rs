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
    base_app().merge(http).nest("/rpc", build_rpc(plugins, None))
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
    // Host-owned `user.v1.UserService` (M18) is folded into the same `/rpc`
    // router as the plugins. Its admin sweep is gated by the admin-token
    // middleware below, which sets the `AdminAuth` marker the handler reads.
    let host_user = crate::rpc::user_service::UserRpc::from_auth_state(&auth_state);
    let admin_token: Option<Arc<str>> = auth_state.admin_api_token.as_deref().map(Arc::from);
    let rpc = build_rpc(plugins, Some(host_user))
        .layer(axum::middleware::from_fn(
            crate::rpc_guard::require_permissions,
        ))
        .layer(axum::middleware::from_fn_with_state(
            ctx_map,
            crate::rpc_guard::inject_ctx,
        ))
        .layer(axum::middleware::from_fn_with_state(
            admin_token,
            crate::rpc_guard::admin_token,
        ));

    let protected = http
        .nest("/rpc", rpc)
        .route("/api/me", get(auth::me::handler))
        .route(
            "/api/me/locale",
            post(auth::me::update_locale).with_state(auth_state.clone()),
        )
        .route(
            "/api/me/refresh-groups",
            post(auth::me::refresh_groups).with_state(auth_state.clone()),
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

/// M24: in `Precompiled` mode, refuse to boot if `[plugins].enabled` doesn't
/// exactly match the plugin set linked into the binary. In `Source` mode this
/// is a no-op (the deployment built its own binary with `junius build` —
/// `[plugins].enabled` already drives the linked set by definition).
///
/// The error names every missing/extra plugin and points at the two escape
/// hatches (add to `enabled`, or rebuild from source) so the operator can
/// recover without grepping for what changed.
fn validate_bundle_compatibility(
    mode: junius_manifest::PlatformMode,
    enabled: &[String],
    plugins: &[Box<dyn Plugin>],
) -> anyhow::Result<()> {
    if !matches!(mode, junius_manifest::PlatformMode::Precompiled) {
        return Ok(());
    }
    let bundled: Vec<&str> = plugins.iter().map(|p| p.metadata().name).collect();
    match bundle_mismatch_message(&bundled, enabled) {
        None => Ok(()),
        Some(msg) => Err(anyhow::anyhow!(msg)),
    }
}

/// Pure helper: returns `None` when `bundled` and `enabled` carry the same
/// set of names, or `Some(msg)` describing the symmetric diff. Extracted
/// from [`validate_bundle_compatibility`] so the diff logic is testable
/// without standing up `Plugin` fixtures.
fn bundle_mismatch_message(bundled: &[&str], enabled: &[String]) -> Option<String> {
    use std::collections::BTreeSet;
    use std::fmt::Write as _;
    let bundled_set: BTreeSet<&str> = bundled.iter().copied().collect();
    let enabled_set: BTreeSet<&str> = enabled.iter().map(String::as_str).collect();
    let missing: Vec<&str> = bundled_set.difference(&enabled_set).copied().collect();
    let extra: Vec<&str> = enabled_set.difference(&bundled_set).copied().collect();
    if missing.is_empty() && extra.is_empty() {
        return None;
    }
    let mut msg = String::from(
        "precompiled image: [plugins].enabled does not match the bundled plugin set\n",
    );
    if !extra.is_empty() {
        let _ = writeln!(
            msg,
            "  enabled but not bundled: {} \
             (remove from [plugins].enabled, or rebuild from source with these plugins available)",
            extra.join(", ")
        );
    }
    if !missing.is_empty() {
        let _ = writeln!(
            msg,
            "  bundled but not enabled: {} \
             (add to [plugins].enabled — precompiled images require an exact match)",
            missing.join(", ")
        );
    }
    Some(msg)
}

/// Trusted-capability allowlist (M18). The `platform.admin` capability grants
/// cross-schema mutation rights on `platform.group*`/`platform.user_role*` via
/// the `PlatformAdminApi` accessor. Only the blessed first-party plugin is
/// allowed to declare it; anyone else fails the host boot with a clear
/// diagnostic so a malicious / mistaken declaration can't slip through `junius
/// check`'s manifest gate.
const TRUSTED_CAPABILITIES: &[(&str, &str)] = &[("platform.admin", "admin")];

fn enforce_trusted_capabilities(plugins: &[Box<dyn Plugin>]) -> anyhow::Result<()> {
    for plugin in plugins {
        let meta = plugin.metadata();
        for cap in meta.capabilities {
            if let Some((_, allowed_plugin)) =
                TRUSTED_CAPABILITIES.iter().find(|(name, _)| name == cap)
                && *allowed_plugin != meta.name
            {
                anyhow::bail!(
                    "plugin {:?} declares trusted capability {:?}, but it is reserved for {:?}",
                    meta.name,
                    cap,
                    allowed_plugin,
                );
            }
        }
    }
    Ok(())
}

/// Aggregate every plugin's declared `[permissions]` into one catalogue the
/// admin plugin's `PermissionCatalogService` returns. The catalogue lives in
/// `HostInfra::admin_catalogue` and is plumbed to each plugin's
/// `PlatformAdminApi` (though only the admin plugin's `platform.admin`
/// capability lets it call `permission_catalogue()`).
fn build_permission_catalogue(
    plugins: &[Box<dyn Plugin>],
) -> Vec<junius_sdk::PluginPermissionsSummary> {
    plugins
        .iter()
        .map(|plugin| {
            let meta = plugin.metadata();
            junius_sdk::PluginPermissionsSummary {
                plugin: meta.name.to_string(),
                display_name: meta.display_name.to_string(),
                permissions: meta
                    .permissions
                    .iter()
                    .map(|p| junius_sdk::PluginPermissionEntry {
                        name: p.name.to_string(),
                        description: p.description.to_string(),
                    })
                    .collect(),
            }
        })
        .collect()
}

/// Fold every plugin's Connect services — plus the host's own
/// `user.v1.UserService` (M18), when `host_user` is provided — into one
/// `connectrpc` router and convert it to an axum router (mounted under `/rpc`
/// by the caller). The resource-less [`build_app`] passes `None`.
fn build_rpc(plugins: &[Box<dyn Plugin>], host_user: Option<crate::rpc::user_service::UserRpc>) -> Router {
    use crate::rpc::proto::user::v1::UserServiceExt as _;
    let mut router = connectrpc::Router::new();
    for plugin in plugins {
        router = plugin.register_rpc(router);
    }
    if let Some(user) = host_user {
        router = Arc::new(user).register(router);
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

/// M24 — operational healthcheck endpoint, served on both topologies.
/// Returns 200 + `{"status":"ok"}` so container orchestrators and the M24
/// image `HEALTHCHECK` directive can probe without needing auth.
async fn healthz() -> impl axum::response::IntoResponse {
    axum::Json(serde_json::json!({"status": "ok"}))
}

#[cfg(not(feature = "embed-frontend"))]
fn base_app() -> Router {
    // M23 — headless mode: every browser route returns a structured 404 so a
    // misrouted request (someone hitting the API tier directly when they
    // should be hitting the FE container) is obvious. API/RPC/auth routes
    // are merged on top of this and continue to match first.
    Router::new()
        .route("/healthz", get(healthz))
        .fallback(axum::routing::any(headless_not_found))
}

#[cfg(not(feature = "embed-frontend"))]
async fn headless_not_found(uri: axum::http::Uri) -> impl axum::response::IntoResponse {
    use axum::http::StatusCode;
    let body = serde_json::json!({
        "error": "not_found",
        "code": "FRONTEND_NOT_EMBEDDED",
        "message": "this juniusd was built without the embedded SPA; \
                    browser routes are served by the M23 SSR FE container",
        "path": uri.path(),
    });
    (StatusCode::NOT_FOUND, axum::Json(body))
}

#[cfg(feature = "embed-frontend")]
fn base_app() -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .merge(crate::static_assets::router())
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

    // M24: in precompiled mode (image entrypoint sets JUNIUS_MODE=precompiled,
    // or the toml's `[build] mode = "precompiled"`), the deployment's
    // `[plugins].enabled` list must exactly match the bundled plugin set.
    // Mismatches fail fast with a clear error before we touch the DB.
    validate_bundle_compatibility(config.build_mode, &config.enabled_plugins, &plugins)?;

    let db = DbBootstrap::connect(&resolved.database_url).await?;
    let platform_pool = db.platform_pool().clone();
    let pools = db
        .build_plugin_pools(
            &resolved.database_url,
            &resolved.role_password_secret,
            &plugins,
        )
        .await?;

    // M18 Stage D: declarative provisioning, hash-guarded. With
    // `auto_apply_on_boot = true` (default), a restart with an unchanged
    // `[provisioning]` block is one SELECT against `platform.provisioning_state`
    // and zero writes; a changed block reconciles in place. The CLI's
    // `junius provision apply` runs the same code path against the same hash
    // row, so a deployment that bootstraps via the CLI and then restarts
    // converges without a second apply.
    if let Some(provisioning) = &config.provisioning {
        if provisioning.auto_apply_on_boot {
            match junius_provision::apply(&platform_pool, provisioning, false).await {
                Ok(outcome) if outcome.unchanged => {
                    tracing::info!(
                        hash = &outcome.hash[..16],
                        "provisioning unchanged; skipped"
                    );
                }
                Ok(outcome) => {
                    tracing::info!(
                        hash = &outcome.hash[..16],
                        groups = outcome.groups,
                        user_roles = outcome.user_roles,
                        user_role_assignments = outcome.user_role_assignments,
                        oidc_mappings = outcome.oidc_mappings,
                        "provisioning applied",
                    );
                }
                Err(e) => {
                    tracing::error!(error = %e, "provisioning auto-apply failed");
                    return Err(anyhow::anyhow!("provisioning auto-apply failed: {e}"));
                }
            }
        }
    }

    // Host-global infra clients (job backend, object stores, email transport,
    // OTel metric sink) — built once, shared across plugins via `build_ctx`.
    // M18: also enforce the trusted-capability allowlist + build the
    // permission catalogue the admin plugin exposes to its UI.
    enforce_trusted_capabilities(&plugins)?;
    let admin_catalogue = build_permission_catalogue(&plugins);
    let infra = HostInfra::build(resolved, config.provisioning.as_ref(), metric_sink)
        .await?
        .with_admin_catalogue(std::sync::Arc::new(admin_catalogue));

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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::bundle_mismatch_message;

    #[test]
    fn no_mismatch_when_sets_match() {
        let bundled = ["admin", "events"];
        let enabled = vec!["admin".to_owned(), "events".to_owned()];
        assert!(bundle_mismatch_message(&bundled, &enabled).is_none());
    }

    #[test]
    fn order_does_not_matter() {
        let bundled = ["events", "admin"];
        let enabled = vec!["admin".to_owned(), "events".to_owned()];
        assert!(bundle_mismatch_message(&bundled, &enabled).is_none());
    }

    #[test]
    fn reports_extras_and_missing_with_recovery_hints() {
        let bundled = ["admin", "events"];
        let enabled = vec!["events".to_owned(), "ghost".to_owned()];
        let msg = bundle_mismatch_message(&bundled, &enabled).expect("mismatch");
        assert!(msg.contains("enabled but not bundled: ghost"), "got: {msg}");
        assert!(msg.contains("bundled but not enabled: admin"), "got: {msg}");
        assert!(msg.contains("rebuild from source"), "got: {msg}");
        assert!(
            msg.contains("precompiled images require an exact match"),
            "got: {msg}"
        );
    }
}
