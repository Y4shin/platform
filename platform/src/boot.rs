//! Plugin lifecycle orchestration: assemble each plugin's request-independent
//! [`PluginResourceCtx`] from the host's pools, and run `on_startup` /
//! `on_shutdown` with a caller-less [`PluginResources`] built from it.

use junius_sdk::{
    AuditEmitter, Auth, Authz, Groups, Locale, Localizer, LocalizerBuilder, Plugin, PluginDb,
    PluginMetadata, PluginResourceCtx, PluginResources, Telemetry, Users,
};
use sqlx::PgPool;

use crate::config::PluginRuntime;
use crate::db::PluginPools;
use crate::infra::HostInfra;

/// Build the request-independent context for one plugin. The `auth` handle is
/// caller-less here; the per-request extractor attaches the current user. The
/// plugin's declared capabilities (from `meta`) gate the `email`/`jobs`/`storage`
/// handles, and the host's metrics sink (from `infra`) is wired into telemetry.
pub fn build_ctx(
    meta: &'static PluginMetadata,
    db: PluginDb,
    platform_pool: &PgPool,
    runtime: &PluginRuntime,
    infra: &HostInfra,
    localizer: &Localizer,
) -> PluginResourceCtx {
    PluginResourceCtx {
        config: runtime.config.clone(),
        telemetry: Telemetry::with_sink(meta.name, infra.metric_sink.clone()),
        db,
        auth: Auth::new(platform_pool.clone()),
        users: Users::new(platform_pool.clone()),
        groups: Groups::new(platform_pool.clone()),
        audit: AuditEmitter::new(platform_pool.clone()),
        authz: Authz::new(platform_pool.clone()),
        email: infra.email_handle(meta.name, meta.capabilities),
        jobs: infra.jobs_handle(platform_pool, meta.name, meta.capabilities),
        storage: infra.storage_handle(platform_pool, meta.name, meta.capabilities),
        localizer: localizer.clone(),
        secrets: runtime.secrets.clone(),
        capabilities: meta.capabilities,
    }
}

/// Build a single shared [`Localizer`] from every plugin's `register_i18n` plus
/// the deployment's configured default locale. Called once at host boot; the
/// result is cloned into each plugin's `PluginResourceCtx`.
pub fn build_localizer(plugins: &[Box<dyn Plugin>], default_locale: Locale) -> Localizer {
    let mut builder = LocalizerBuilder::new(default_locale);
    for plugin in plugins {
        plugin.register_i18n(&mut builder);
    }
    builder.build()
}

/// Run `on_startup` for every plugin in registration order. Fails on the first
/// error; the binary aborts on failure.
pub async fn run_startup(
    plugins: &[Box<dyn Plugin>],
    pools: &PluginPools,
    platform_pool: &PgPool,
    runtimes: &std::collections::BTreeMap<String, PluginRuntime>,
    infra: &HostInfra,
    localizer: &Localizer,
) -> anyhow::Result<()> {
    for plugin in plugins {
        let meta = plugin.metadata();
        let name = meta.name;
        let Some(db) = pools.get(name) else {
            anyhow::bail!("no database pool was built for plugin {name}");
        };
        let runtime = runtimes.get(name).cloned().unwrap_or_default();
        let ctx = build_ctx(meta, db, platform_pool, &runtime, infra, localizer);
        let resources = PluginResources::from_ctx(&ctx, None);
        plugin
            .on_startup(&resources)
            .await
            .map_err(|e| anyhow::anyhow!("plugin {name} on_startup failed: {e}"))?;
    }
    Ok(())
}

/// Run `on_shutdown` for every plugin in reverse registration order. Errors are
/// logged but not propagated — shutdown is best-effort.
pub async fn run_shutdown(
    plugins: &[Box<dyn Plugin>],
    pools: &PluginPools,
    platform_pool: &PgPool,
    runtimes: &std::collections::BTreeMap<String, PluginRuntime>,
    infra: &HostInfra,
    localizer: &Localizer,
) {
    for plugin in plugins.iter().rev() {
        let meta = plugin.metadata();
        let name = meta.name;
        let Some(db) = pools.get(name) else {
            continue;
        };
        let runtime = runtimes.get(name).cloned().unwrap_or_default();
        let ctx = build_ctx(meta, db, platform_pool, &runtime, infra, localizer);
        let resources = PluginResources::from_ctx(&ctx, None);
        if let Err(e) = plugin.on_shutdown(&resources).await {
            tracing::error!(plugin = name, error = %e, "on_shutdown failed");
        }
    }
}
