//! `platform.toml` schema.

use serde::Deserialize;
use std::collections::BTreeMap;

use crate::error::{ManifestError, ValidationReport};
use crate::validate;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformManifest {
    pub source: SourceConfig,
    pub plugins: PluginsConfig,
    #[serde(default)]
    pub config: BTreeMap<String, toml::Value>,
    /// M18 Stage D — declarative starting state for groups/roles/perms/
    /// memberships/user-roles/OIDC mappings. Applied by
    /// `junius provision apply` (and at host boot, hash-guarded).
    #[serde(default)]
    pub provisioning: Option<crate::provisioning::ProvisioningConfig>,
    /// M23 — deployment build knobs: today, whether `junius build`
    /// produces a binary with the SPA embedded (default) or a headless
    /// API-only binary that pairs with the M23 SSR FE container.
    #[serde(default)]
    pub build: BuildConfig,
}

/// `[build]` block on `platform.toml` (M23).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildConfig {
    /// How the deployment ships its frontend. `"embedded"` (default) bundles
    /// the SPA into `juniusd` via `rust-embed`; `"none"` produces a
    /// headless API-only binary that returns a structured 404 on browser
    /// routes — pair it with the M23 SSR FE container.
    #[serde(default)]
    pub frontend: FrontendDelivery,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FrontendDelivery {
    /// SPA assets embedded in the host binary (the today-default).
    #[default]
    Embedded,
    /// No FE in the host binary; an external SSR/static container serves it.
    None,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceConfig {
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub rev: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}

// NOTE: no `deny_unknown_fields` here — serde does not support it alongside
// `#[serde(flatten)]`, and the flatten *is* the catch-all for `[plugins.<name>]`
// override tables.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginsConfig {
    pub enabled: Vec<String>,
    /// Per-plugin override tables: `[plugins.<name>]`. Stored verbatim.
    #[serde(flatten)]
    pub overrides: BTreeMap<String, toml::Value>,
}

impl PlatformManifest {
    /// Parse from TOML text.
    pub fn parse(src: &str) -> Result<Self, ManifestError> {
        Ok(toml::from_str::<Self>(src)?)
    }

    /// Run semantic validation. Returns an empty report on success.
    pub fn validate(&self) -> ValidationReport {
        let mut report = ValidationReport::default();
        validate::platform(self, &mut report);
        report
    }
}
