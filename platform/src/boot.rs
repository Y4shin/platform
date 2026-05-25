//! Plugin lifecycle orchestration: assemble each plugin's request-independent
//! [`PluginResourceCtx`] from the host's pools, and run `on_startup` /
//! `on_shutdown` with a caller-less [`PluginResources`] built from it.

use junius_sdk::{
    AuditEmitter, Auth, Authz, Plugin, PluginDb, PluginResourceCtx, PluginResources, Telemetry,
    Users,
};
use sqlx::PgPool;

use crate::config::PluginRuntime;
use crate::db::PluginPools;

/// Build the request-independent context for one plugin. The `auth` handle is
/// caller-less here; the per-request extractor attaches the current user.
pub fn build_ctx(
    name: &'static str,
    db: PluginDb,
    platform_pool: &PgPool,
    runtime: &PluginRuntime,
) -> PluginResourceCtx {
    PluginResourceCtx::new(
        runtime.config.clone(),
        Telemetry::new(name),
        db,
        Auth::new(platform_pool.clone()),
        Users::new(platform_pool.clone()),
        AuditEmitter::new(platform_pool.clone()),
        Authz::new(platform_pool.clone()),
        runtime.secrets.clone(),
    )
}

/// Run `on_startup` for every plugin in registration order. Fails on the first
/// error; the binary aborts on failure.
pub async fn run_startup(
    plugins: &[Box<dyn Plugin>],
    pools: &PluginPools,
    platform_pool: &PgPool,
    runtimes: &std::collections::BTreeMap<String, PluginRuntime>,
) -> anyhow::Result<()> {
    for plugin in plugins {
        let name = plugin.metadata().name;
        let Some(db) = pools.get(name) else {
            anyhow::bail!("no database pool was built for plugin {name}");
        };
        let runtime = runtimes.get(name).cloned().unwrap_or_default();
        let ctx = build_ctx(name, db, platform_pool, &runtime);
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
) {
    for plugin in plugins.iter().rev() {
        let name = plugin.metadata().name;
        let Some(db) = pools.get(name) else {
            continue;
        };
        let runtime = runtimes.get(name).cloned().unwrap_or_default();
        let ctx = build_ctx(name, db, platform_pool, &runtime);
        let resources = PluginResources::from_ctx(&ctx, None);
        if let Err(e) = plugin.on_shutdown(&resources).await {
            tracing::error!(plugin = name, error = %e, "on_shutdown failed");
        }
    }
}
