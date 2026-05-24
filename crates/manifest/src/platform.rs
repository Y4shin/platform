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
