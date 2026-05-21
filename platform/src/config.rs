//! Host-level runtime configuration.
//!
//! The platform host has exactly **one** source of runtime config: a
//! [`HostConfig`] value constructed at boot and threaded through the call
//! graph. No code path reads `std::env::var` for deployment knobs — config is
//! always declarative.
//!
//! At M02 [`HostConfig::default`] returns the in-tree defaults and that's all
//! that's wired up. M11 (deployment workflow) extends this with
//! [`HostConfig::load_from_toml`] that parses the deployment's `platform.toml`
//! `[config]` block and produces a fully-resolved value (env-var indirection
//! for secrets is also handled there, in one place).
//!
//! The single tolerated exception is `RUST_LOG`, read by `tracing-subscriber`
//! itself — a developer-debug knob, not deployment config.

use std::net::SocketAddr;

#[derive(Clone, Debug)]
pub struct HostConfig {
    /// The TCP address `juniusd` binds for HTTP.
    pub bind_addr: SocketAddr,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            // 127.0.0.1:18080 — local default. 8080 collides with too many
            // common dev tools. Production deployments override this via the
            // `[config]` block of `platform.toml` once M11 wires that up.
            bind_addr: SocketAddr::from(([127, 0, 0, 1], 18080)),
        }
    }
}
