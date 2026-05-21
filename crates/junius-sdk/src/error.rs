//! Errors returned from the SDK surface.

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    /// A plugin attempted to use a capability handle it did not declare in its
    /// manifest's `[requires].capabilities`. Audit-only in v1 (logged + returned
    /// as a runtime error); compile-time enforcement is deferred.
    #[error("capability not declared: {0}")]
    CapabilityNotDeclared(&'static str),

    /// Plugin config lookup or deserialisation failure.
    #[error("config error: {0}")]
    Config(String),

    /// Escape hatch for anything a plugin's own code wants to bubble up.
    #[error(transparent)]
    External(#[from] anyhow::Error),
}
