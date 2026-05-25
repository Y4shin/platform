//! `platform` — the Junius host (binary name: `juniusd`).
//!
//! The crate is structured as both a library and a binary so integration tests
//! can drive [`server::build_app`] directly instead of spawning the binary.

pub mod auth;
pub mod boot;
pub mod config;
pub mod db;
pub mod generated;
pub mod infra;
pub mod plugin_registry;
pub mod rpc_guard;
pub mod server;

#[cfg(feature = "embed-frontend")]
mod static_assets;
