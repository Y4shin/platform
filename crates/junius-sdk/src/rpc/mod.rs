//! Connect-RPC HTTP unary protocol — server side.
//!
//! Plugins build their RPC surface by chaining `.unary(...)` calls on a
//! [`ServiceBuilder`] and returning the resulting [`axum::Router`] from
//! [`Plugin::rpc_routes`](crate::Plugin::rpc_routes). The host nests all
//! plugin routers under `/rpc/`; the Connect-Web client targets
//! `<baseUrl>/<service.typeName>/<method>` where `baseUrl` is `/rpc`.
//!
//! Both `application/proto` (binary, the Connect default) and
//! `application/json` content types are accepted. Permission enforcement
//! lands in M07 via a layer on top of this builder.

mod builder;
mod codec;
mod error;

pub use builder::{RpcResult, ServiceBuilder};
pub use error::{RpcCode, RpcError};
