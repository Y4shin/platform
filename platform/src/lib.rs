//! `platform` — the Junius host (binary name: `juniusd`).
//!
//! The crate is structured as both a library and a binary so integration tests
//! can drive [`server::build_app`] directly instead of spawning the binary.

pub mod boot;
pub mod config;
pub mod plugin_registry;
pub mod server;

#[cfg(feature = "embed-frontend")]
mod static_assets;
