//! `plugin.toml` schema.

use serde::Deserialize;
use std::collections::BTreeMap;

use crate::error::{ManifestError, ValidationReport};
use crate::validate;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    pub plugin: PluginIdentity,
    #[serde(default)]
    pub mount: PluginMount,
    #[serde(default)]
    pub dependencies: BTreeMap<String, PluginDep>,
    #[serde(default)]
    pub exposes: PluginExposes,
    #[serde(default)]
    pub permissions: BTreeMap<String, String>,
    #[serde(default)]
    pub requires: PluginRequires,
    /// Typed config schema: `[config.<key>]`. Codegen emits a `Config` struct.
    #[serde(default)]
    pub config: BTreeMap<String, ConfigField>,
    /// Declared secrets: `[secrets.<name>]`. Codegen emits a typed `Secrets`
    /// accessor; the deployment supplies the values.
    #[serde(default)]
    pub secrets: BTreeMap<String, SecretDecl>,
    /// Logical object-storage buckets: `[storage.buckets.<name>]`. The
    /// `plugin_metadata!` macro emits a typed `Bucket` enum from these; the
    /// deployment maps each to a physical bucket in `[config.storage.mapping]`.
    #[serde(default)]
    pub storage: PluginStorageDecl,
}

/// The `[storage]` section of a plugin manifest.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginStorageDecl {
    #[serde(default)]
    pub buckets: BTreeMap<String, BucketDecl>,
}

/// A logical bucket a plugin declares.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BucketDecl {
    #[serde(default)]
    pub description: Option<String>,
}

/// A single typed config field declared by a plugin.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigField {
    #[serde(rename = "type")]
    pub ty: ConfigType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<toml::Value>,
    #[serde(default)]
    pub description: Option<String>,
}

/// The scalar types a config field may declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigType {
    String,
    Integer,
    Boolean,
    Float,
}

/// A secret a plugin requires. The value is provided by the deployment, never
/// here.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretDecl {
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginIdentity {
    pub name: String,
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub manifest_schema: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginMount {
    pub route_prefix: Option<String>,
    pub rpc_prefix: Option<String>,
    pub http_prefix: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginDep {
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub tables: Vec<String>,
    #[serde(default)]
    pub rpc_methods: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginExposes {
    #[serde(default)]
    pub components: BTreeMap<String, ExposedComponent>,
    #[serde(default)]
    pub tables: BTreeMap<String, ExposedTable>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExposedComponent {
    pub module: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExposedTable {
    pub schema: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginRequires {
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl PluginManifest {
    /// Parse from TOML text. Does not run semantic validation; call [`Self::validate`].
    pub fn parse(src: &str) -> Result<Self, ManifestError> {
        Ok(toml::from_str::<Self>(src)?)
    }

    /// Fill in mount-prefix defaults derived from `plugin.name`.
    #[must_use]
    pub fn with_defaults(mut self) -> Self {
        let name = &self.plugin.name;
        if self.mount.route_prefix.is_none() {
            self.mount.route_prefix = Some(format!("/p/{name}"));
        }
        if self.mount.rpc_prefix.is_none() {
            self.mount.rpc_prefix = Some(format!("/rpc/{name}"));
        }
        if self.mount.http_prefix.is_none() {
            self.mount.http_prefix = Some(format!("/h/{name}"));
        }
        self
    }

    /// Run semantic validation. Returns an empty report on success.
    pub fn validate(&self) -> ValidationReport {
        let mut report = ValidationReport::default();
        validate::plugin(self, &mut report);
        report
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests panic on unexpected failures")]
mod tests {
    use super::*;

    const HEAD: &str =
        "[plugin]\nname = \"hello\"\ndisplay_name = \"Hello\"\nmanifest_schema = 1\n";

    #[test]
    fn parses_and_validates_storage_buckets() {
        let m = PluginManifest::parse(&format!(
            "{HEAD}[storage.buckets.attachments]\ndescription = \"Note files\"\n"
        ))
        .unwrap();
        assert!(m.storage.buckets.contains_key("attachments"));
        assert!(m.validate().is_ok(), "{:?}", m.validate().issues);
    }

    #[test]
    fn rejects_malformed_bucket_name() {
        let m = PluginManifest::parse(&format!("{HEAD}[storage.buckets.Bad-Name]\n")).unwrap();
        let report = m.validate();
        assert!(
            report
                .issues
                .iter()
                .any(|i| i.code == "STORAGE.BUCKET.NAME"),
            "expected STORAGE.BUCKET.NAME (got {:?})",
            report.issues
        );
    }
}
