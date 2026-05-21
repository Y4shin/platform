//! The `Plugin` trait that every plugin crate implements once.
//!
//! Object-safe (via `async_trait`) so the host can hold a heterogenous
//! `Vec<Box<dyn Plugin>>`.

use axum::Router;

use crate::error::PluginError;
use crate::metadata::PluginMetadata;
use crate::resources::PluginResources;

#[async_trait::async_trait]
pub trait Plugin: Send + Sync + 'static {
    /// `'static` view of the plugin's manifest. Produced by the
    /// `plugin_metadata!()` macro.
    fn metadata(&self) -> &'static PluginMetadata;

    /// Build the plugin's routes against its host-provided resources. The
    /// returned router is unprefixed; the host nests it under
    /// `metadata().mount.{http,rpc}_prefix`.
    fn routes(&self, resources: PluginResources) -> Router;

    /// Called once per plugin after migrations and before the server starts
    /// accepting traffic. Default no-op.
    async fn on_startup(&self, _resources: &PluginResources) -> Result<(), PluginError> {
        Ok(())
    }

    /// Called once per plugin during graceful shutdown, in reverse
    /// registration order. Default no-op.
    async fn on_shutdown(&self, _resources: &PluginResources) -> Result<(), PluginError> {
        Ok(())
    }

    // `jobs()` lands in M10 alongside the background job system.
}
