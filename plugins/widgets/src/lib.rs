//! `widgets` — a UI-only plugin (M09). It exists to demonstrate cross-plugin
//! *component* sharing: it exposes a single React component (`VenuePicker`) that
//! other plugins consume through the typed component registry. It has no
//! database, no RPC, and no resources — the minimal Rust surface below lets the
//! host register it like any other plugin (metadata, permissions, a `/ping`
//! HTTP route) without `register_rpc` (the trait default is a no-op).

use axum::{Router, routing::get};
use junius_sdk::{Plugin, PluginMetadata};

junius_sdk::plugin_metadata!();

pub struct WidgetsPlugin;

impl WidgetsPlugin {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for WidgetsPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Plugin for WidgetsPlugin {
    fn metadata(&self) -> &'static PluginMetadata {
        &METADATA
    }

    fn routes(&self) -> Router {
        Router::new().route("/ping", get(|| async { "pong" }))
    }
}
