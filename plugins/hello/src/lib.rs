//! `hello` — the smoke-test plugin. Implements the smallest possible
//! `Plugin`: no DB, no permissions, no RPC, no frontend — just a single HTTP
//! route `GET /ping → "pong"`. M03's existence proof that a manifest, a Rust
//! crate, and `junius sync` compose into a working host binary.

use async_trait::async_trait;
use axum::{Router, routing::get};
use junius_sdk::{Plugin, PluginMetadata, PluginResources};

junius_sdk::plugin_metadata!();

pub struct HelloPlugin;

impl HelloPlugin {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for HelloPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for HelloPlugin {
    fn metadata(&self) -> &'static PluginMetadata {
        &METADATA
    }

    fn routes(&self, _resources: PluginResources) -> Router {
        Router::new().route("/ping", get(|| async { "pong" }))
    }
}
