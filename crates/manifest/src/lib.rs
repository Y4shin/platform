//! On-disk schemas for `plugin.toml` and `platform.toml`, plus semantic
//! validation. Pure data types and pure functions; no IO beyond `&str` input.

pub mod error;
pub mod grants;
pub mod platform;
pub mod plugin;
pub mod secrets;
pub mod validate;

pub use error::{ManifestError, SecretError, Severity, ValidationIssue, ValidationReport};
pub use grants::{
    DepGrant, RoleGrant, compute_grants, derive_role_password, emit_grant_sql, role_name,
};
pub use platform::{PlatformManifest, PluginsConfig, SourceConfig};
pub use plugin::{
    ConfigField, ConfigType, ExposedComponent, ExposedTable, PluginDep, PluginExposes,
    PluginIdentity, PluginManifest, PluginMount, PluginRequires, SecretDecl,
};
pub use secrets::{ResolvedConfig, SecretRef, resolve_config};
