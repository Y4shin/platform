//! The `Plugin` trait that every plugin crate implements once.
//!
//! Object-safe (via `async_trait`) so the host can hold a heterogenous
//! `Vec<Box<dyn Plugin>>`.

use axum::Router;

use crate::error::PluginError;
use crate::jobs::JobHandler;
use crate::localizer::LocalizerBuilder;
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

    /// Register the plugin's Connect-RPC services into the shared `connectrpc`
    /// router and return it. Default: no RPC. The host folds every plugin's
    /// services into **one** `connectrpc::Router` (keyed by the proto-package-
    /// scoped service FQN) and serves it under `/rpc`; it injects the correct
    /// per-plugin `PluginResourceCtx` per request (by service FQN) so handlers
    /// read it via `RequestContext::extensions` (`PluginContext::from_rpc`).
    /// Implement as `Arc::new(MyService).register(router)`.
    fn register_rpc(&self, router: connectrpc::Router) -> connectrpc::Router {
        router
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

    /// Background-job handlers the plugin registers (M10). The host worker
    /// dispatches each by its [`Job::NAME`](crate::jobs::Job::NAME) with the
    /// plugin's own (caller-less) [`PluginResources`]. Default: none.
    fn jobs(&self) -> Vec<JobHandler> {
        Vec::new()
    }

    /// Install the plugin's i18n catalog into the host's [`LocalizerBuilder`]
    /// (M14). Plugins with no strings (or no catalog yet) inherit the default
    /// no-op; the `junius_sdk::i18n_catalog!()` macro generates a
    /// `catalog::register(b)` function plugins forward to here.
    fn register_i18n(&self, _builder: &mut LocalizerBuilder) {}
}
