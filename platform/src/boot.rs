//! Plugin lifecycle orchestration: build per-plugin `PluginResources`, call
//! `on_startup` / `on_shutdown` hooks. Used by the binary's `main` and by
//! integration tests.

use junius_sdk::{Plugin, PluginConfig, PluginResources, Telemetry};

/// Construct a fresh `PluginResources` for the plugin named `name`. At M02
/// every plugin gets an empty config; later milestones will look up
/// `[plugins.<name>]` from the loaded deployment config and add DB / storage /
/// jobs / etc. fields.
pub fn build_resources_for(name: &'static str) -> PluginResources {
    PluginResources::new(PluginConfig::empty(), Telemetry::new(name))
}

/// Run `on_startup` for every plugin in registration order. Fails on the
/// first error; the binary aborts on failure.
pub async fn run_startup(plugins: &[Box<dyn Plugin>]) -> anyhow::Result<()> {
    for plugin in plugins {
        let metadata = plugin.metadata();
        let resources = build_resources_for(metadata.name);
        plugin
            .on_startup(&resources)
            .await
            .map_err(|e| anyhow::anyhow!("plugin {} on_startup failed: {e}", metadata.name))?;
    }
    Ok(())
}

/// Run `on_shutdown` for every plugin in reverse registration order. Errors
/// are logged but not propagated — shutdown is best-effort.
pub async fn run_shutdown(plugins: &[Box<dyn Plugin>]) {
    for plugin in plugins.iter().rev() {
        let metadata = plugin.metadata();
        let resources = build_resources_for(metadata.name);
        if let Err(e) = plugin.on_shutdown(&resources).await {
            tracing::error!(plugin = metadata.name, error = %e, "on_shutdown failed");
        }
    }
}
