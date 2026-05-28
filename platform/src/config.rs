//! Host-level runtime configuration.
//!
//! [`HostConfig`] is constructed once at boot and threaded through the call
//! graph. [`HostConfig::default`] gives the in-tree defaults with no database
//! (used by resource-less integration tests); [`HostConfig::load_from_toml`]
//! parses a deployment `platform.toml`, resolving the `[config]` block and each
//! `[plugins.<name>]`'s `config`/`secrets` (with `env:` indirection) into typed
//! per-plugin runtime resources.
//!
//! M06 pulls in the subset of `[config]` the host needs now (DB, OIDC, session,
//! role-password secret); the broader deployment-config story is M11.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr as _;

use junius_manifest::{
    PlatformManifest, ProvisioningConfig, ResolvedConfig, SecretRef, secrets,
};
use junius_sdk::{PluginConfig, SecretStore};

/// Per-plugin runtime resources resolved from `[plugins.<name>]`.
#[derive(Clone, Default)]
pub struct PluginRuntime {
    pub config: PluginConfig,
    pub secrets: SecretStore,
}

#[derive(Clone)]
pub struct HostConfig {
    /// The TCP address `juniusd` binds for HTTP.
    pub bind_addr: SocketAddr,
    /// Resolved `[config]` block. `None` for [`HostConfig::default`] (no DB).
    pub resolved: Option<ResolvedConfig>,
    /// Per-plugin config + secrets, keyed by plugin name.
    pub plugins: BTreeMap<String, PluginRuntime>,
    /// M18 Stage D: the resolved `[provisioning]` block (with `file = "…"`
    /// pointer already loaded). `None` when the deployment declares none.
    pub provisioning: Option<ProvisioningConfig>,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            // 127.0.0.1:18080 — local default; 8080 collides with too many dev tools.
            bind_addr: SocketAddr::from(([127, 0, 0, 1], 18080)),
            resolved: None,
            plugins: BTreeMap::new(),
            provisioning: None,
        }
    }
}

impl HostConfig {
    /// Parse a deployment `platform.toml`: resolve its `[config]` block and each
    /// enabled plugin's `[plugins.<name>]` config/secrets. Also resolves the
    /// `[provisioning]` block's optional `file = "…"` pointer so the host can
    /// run the M18 Stage D auto-apply pass at boot without re-reading the TOML.
    pub fn load_from_toml(path: &Path) -> anyhow::Result<Self> {
        let src = std::fs::read_to_string(path)?;
        let manifest = PlatformManifest::parse(&src)?;
        let resolved = secrets::resolve_config(&manifest.config, &env_lookup())?;
        let bind_addr = match &resolved.bind_addr {
            Some(addr) => addr.parse()?,
            None => SocketAddr::from(([127, 0, 0, 1], 18080)),
        };
        let plugins = resolve_plugin_runtimes(&manifest)?;
        let deployment_dir: PathBuf = path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        let provisioning = manifest
            .provisioning
            .clone()
            .map(|p| p.resolve_file(&deployment_dir))
            .transpose()?;
        Ok(Self {
            bind_addr,
            resolved: Some(resolved),
            plugins,
            provisioning,
        })
    }

    /// The runtime resources for `name`, or empty defaults if the deployment
    /// declared none.
    #[must_use]
    pub fn plugin_runtime(&self, name: &str) -> PluginRuntime {
        self.plugins.get(name).cloned().unwrap_or_default()
    }
}

fn resolve_plugin_runtimes(
    manifest: &PlatformManifest,
) -> anyhow::Result<BTreeMap<String, PluginRuntime>> {
    let mut out = BTreeMap::new();
    for (name, value) in &manifest.plugins.overrides {
        let Some(table) = value.as_table() else {
            continue;
        };

        let config = table
            .get("config")
            .and_then(|v| v.as_table())
            .cloned()
            .map(PluginConfig::from_table)
            .unwrap_or_default();

        let mut secret_pairs = Vec::new();
        if let Some(secret_table) = table.get("secrets").and_then(|v| v.as_table()) {
            for (secret_name, source) in secret_table {
                let source = source.as_str().ok_or_else(|| {
                    anyhow::anyhow!("plugins.{name}.secrets.{secret_name} must be a string")
                })?;
                let value = SecretRef::from_str(source)?.resolve(&env_lookup())?;
                secret_pairs.push((secret_name.clone(), value));
            }
        }

        out.insert(
            name.clone(),
            PluginRuntime {
                config,
                secrets: SecretStore::from_pairs(secret_pairs),
            },
        );
    }
    Ok(out)
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
