//! `hello` — the smoke-test plugin. M03 introduced the HTTP-only shape
//! (`GET /ping → "pong"`). M05 adds a Connect-RPC service:
//! `hello.v1.HelloService.Greet` returns a greeting string for the supplied
//! name. Together they exercise both `Plugin::routes()` (plain HTTP) and
//! `Plugin::rpc_routes()` (the Connect protocol path).

use async_trait::async_trait;
use axum::{Router, routing::get};
use junius_sdk::rpc::{RpcResult, ServiceBuilder};
use junius_sdk::{Plugin, PluginMetadata, PluginResources};

junius_sdk::plugin_metadata!();

mod pb {
    // prost-build outputs the message types here; see build.rs.
    #![allow(clippy::doc_markdown, missing_docs)]
    include!(concat!(env!("OUT_DIR"), "/hello.v1.rs"));
}

pub use pb::{GreetRequest, GreetResponse};

pub struct HelloPlugin;

impl HelloPlugin {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Business logic for the `Greet` RPC. Pure function for now; once M06
    /// lands DB access, this would route through a repository. The `async`
    /// keyword is here to match `ServiceBuilder::unary`'s handler signature
    /// — Greet has no real awaits yet.
    #[allow(
        clippy::unused_async,
        reason = "matches the async handler signature required by ServiceBuilder::unary"
    )]
    async fn greet(req: GreetRequest) -> RpcResult<GreetResponse> {
        let name = if req.name.is_empty() {
            "world".to_string()
        } else {
            req.name
        };
        Ok(GreetResponse {
            message: format!("Hello, {name}!"),
        })
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

    fn rpc_routes(&self, _resources: PluginResources) -> Router {
        ServiceBuilder::new("hello.v1.HelloService")
            .unary("Greet", Self::greet)
            .into_router()
    }
}
