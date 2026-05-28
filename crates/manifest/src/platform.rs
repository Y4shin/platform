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

/// `[build]` block on `platform.toml` (M23 + M24).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildConfig {
    /// How the deployment ships its frontend. `"embedded"` (default) bundles
    /// the SPA into `juniusd` via `rust-embed`; `"none"` produces a
    /// headless API-only binary that returns a structured 404 on browser
    /// routes — pair it with the M23 SSR FE container.
    #[serde(default)]
    pub frontend: FrontendDelivery,
    /// M24 — how this `juniusd` was built. `"source"` (default) is the
    /// `junius build` path that picks plugins from `[plugins].enabled`;
    /// `"precompiled"` is the bundle-every-plugin image variant, which
    /// asserts at boot that `[plugins].enabled` matches the bundled set
    /// exactly. The image entrypoint sets `JUNIUS_MODE=precompiled` to
    /// override whatever the deployment's toml says (the image is
    /// authoritative about its own mode).
    #[serde(default)]
    pub mode: PlatformMode,
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

/// M24 — how this `juniusd` was built.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlatformMode {
    /// Built from source via `junius build` against a deployment's
    /// `platform.toml`. The plugin set is whatever the deployment
    /// enabled.
    #[default]
    Source,
    /// Built as a precompiled image bundling every monorepo plugin.
    /// `[plugins].enabled` must exactly match the bundled set; mismatches
    /// fail the boot check.
    Precompiled,
}

impl std::str::FromStr for PlatformMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "source" => Ok(Self::Source),
            "precompiled" => Ok(Self::Precompiled),
            other => Err(format!(
                "invalid platform mode {other:?} (expected \"source\" or \"precompiled\")"
            )),
        }
    }
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::str::FromStr as _;

    const BASE: &str = "[source]\npath = \".\"\n\n[plugins]\nenabled = []\n";

    #[test]
    fn build_mode_defaults_to_source() {
        let m = PlatformManifest::parse(BASE).unwrap();
        assert_eq!(m.build.mode, PlatformMode::Source);
        assert_eq!(m.build.frontend, FrontendDelivery::Embedded);
    }

    #[test]
    fn build_mode_precompiled_parses() {
        let src = format!("{BASE}\n[build]\nmode = \"precompiled\"\n");
        let m = PlatformManifest::parse(&src).unwrap();
        assert_eq!(m.build.mode, PlatformMode::Precompiled);
    }

    #[test]
    fn build_mode_invalid_value_rejected() {
        let src = format!("{BASE}\n[build]\nmode = \"hybrid\"\n");
        assert!(PlatformManifest::parse(&src).is_err());
    }

    #[test]
    fn platform_mode_from_str_round_trip() {
        assert_eq!(PlatformMode::from_str("source").unwrap(), PlatformMode::Source);
        assert_eq!(
            PlatformMode::from_str("precompiled").unwrap(),
            PlatformMode::Precompiled
        );
        assert!(PlatformMode::from_str("nonsense").is_err());
    }
}
