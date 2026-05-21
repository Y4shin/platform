//! On-disk schemas for `plugin.toml` and `platform.toml`, plus semantic
//! validation. Pure data types and pure functions; no IO beyond `&str` input.

pub mod error;
pub mod platform;
pub mod plugin;
pub mod validate;

pub use error::{ManifestError, Severity, ValidationIssue, ValidationReport};
pub use platform::{PlatformManifest, PluginsConfig, SourceConfig};
pub use plugin::{
    ExposedComponent, ExposedTable, PluginDep, PluginExposes, PluginIdentity, PluginManifest,
    PluginMount, PluginRequires,
};
