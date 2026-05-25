//! On-disk schemas for `plugin.toml`, `platform.toml`, and `platform.lock`, plus
//! semantic validation. Pure data types and pure functions; no IO beyond `&str`
//! input — except the `file:` secret indirection, which reads from disk (via an
//! injectable reader; see [`secrets::SecretRef::resolve_with`]).

pub mod error;
pub mod grants;
pub mod infra_config;
pub mod lock;
pub mod platform;
pub mod plugin;
pub mod secrets;
pub mod validate;

pub use error::{ManifestError, SecretError, Severity, ValidationIssue, ValidationReport};
pub use grants::{
    DepGrant, RoleGrant, compute_grants, derive_role_password, emit_grant_sql, role_name,
};
pub use infra_config::{
    AuditConfig, EmailConfig, JobsConfig, MailpitConfig, OtelConfig, PhysicalBucket, ResendConfig,
    SmtpConfig, StorageConfig,
};
pub use lock::{DeploymentLock, LockSource};
pub use platform::{PlatformManifest, PluginsConfig, SourceConfig};
pub use plugin::{
    BucketDecl, ConfigField, ConfigType, ExposedComponent, ExposedTable, PluginDep, PluginExposes,
    PluginIdentity, PluginManifest, PluginMount, PluginRequires, PluginStorageDecl, SecretDecl,
};
pub use secrets::{ResolvedConfig, SecretRef, resolve_config};
