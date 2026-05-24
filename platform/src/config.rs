//! Host-level runtime configuration.
//!
//! [`HostConfig`] is constructed once at boot and threaded through the call
//! graph. [`HostConfig::default`] gives the in-tree defaults with no database
//! (used by resource-less integration tests); [`HostConfig::load_from_toml`]
//! parses a deployment `platform.toml`, resolving the `[config]` block (with
//! `env:` secret indirection) into a [`ResolvedConfig`].
//!
//! M06 pulls in the subset of `[config]` the host needs now (DB, OIDC, session,
//! role-password secret); the broader deployment-config story is M11.
//!
//! The single tolerated env read is the `env:` secret indirection here (and
//! `RUST_LOG`, read by `tracing-subscriber` itself).

use std::net::SocketAddr;
use std::path::Path;

use junius_manifest::{PlatformManifest, ResolvedConfig, secrets};

#[derive(Clone)]
pub struct HostConfig {
    /// The TCP address `juniusd` binds for HTTP.
    pub bind_addr: SocketAddr,
    /// Resolved `[config]` block. `None` for [`HostConfig::default`] (no DB).
    pub resolved: Option<ResolvedConfig>,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            // 127.0.0.1:18080 — local default; 8080 collides with too many dev tools.
            bind_addr: SocketAddr::from(([127, 0, 0, 1], 18080)),
            resolved: None,
        }
    }
}

impl HostConfig {
    /// Parse a deployment `platform.toml` and resolve its `[config]` block.
    pub fn load_from_toml(path: &Path) -> anyhow::Result<Self> {
        let src = std::fs::read_to_string(path)?;
        let manifest = PlatformManifest::parse(&src)?;
        let resolved = secrets::resolve_config(&manifest.config, &env_lookup())?;
        let bind_addr = match &resolved.bind_addr {
            Some(addr) => addr.parse()?,
            None => SocketAddr::from(([127, 0, 0, 1], 18080)),
        };
        Ok(Self {
            bind_addr,
            resolved: Some(resolved),
        })
    }
}

/// Lookup closure for `env:` secret indirections — the one sanctioned direct
/// environment read in the host's config path.
fn env_lookup() -> impl Fn(&str) -> Option<String> {
    #[allow(
        clippy::disallowed_methods,
        reason = "resolving env: secret indirections from platform.toml [config]"
    )]
    |var: &str| std::env::var(var).ok()
}
