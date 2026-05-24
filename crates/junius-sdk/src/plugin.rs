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

    /// Build the plugin's plain-HTTP routes. The returned router is unprefixed;
    /// the host nests it under `metadata().mount.http_prefix` (`/h/<plugin>`).
    ///
    /// Handlers obtain host resources per request via the [`PluginResources`]
    /// extractor (the host attaches the per-plugin context to the subtree), so
    /// `routes` itself takes no resources argument. The current caller comes
    /// from `resources.auth.current_user()` or the
    /// [`CurrentUser`](crate::auth::CurrentUser) extractor.
    fn routes(&self) -> Router;

    /// Build the plugin's Connect-RPC routes. Default returns an empty router
    /// for plugins that don't expose RPCs. The host merges every plugin's RPC
    /// router under `/rpc` (a flat namespace — the proto package name is
    /// what scopes services by plugin). Use
    /// [`junius_sdk::rpc::ServiceBuilder`](crate::rpc::ServiceBuilder) to
    /// construct the router.
    fn rpc_routes(&self) -> Router {
        Router::new()
    }

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
