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

    /// The caller is not permitted to perform the operation (e.g. sharing a
    /// resource they don't own). Maps to 403 / `permission_denied`.
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    /// A mutation targeted a row owned by the deployment's `[provisioning]`
    /// block while `lock_managed = true`. The admin UI surfaces this as a
    /// "locked by provisioning" diagnostic; the only way to mutate the row is
    /// to remove its declaration from the TOML and re-run
    /// `junius provision apply`.
    #[error("row is managed by provisioning config: {0}")]
    ManagedByConfig(String),

    /// A database operation failed (host-provided handles: `Auth`, `Users`,
    /// `AuditEmitter`).
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),

    /// Escape hatch for anything a plugin's own code wants to bubble up.
    #[error(transparent)]
    External(#[from] anyhow::Error),
}

/// Error returned by repository methods (M07). Distinct from [`PluginError`] so
/// data-access failures have a focused type that handlers map to an HTTP/RPC
/// error at the boundary.
#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    /// The underlying SQL query failed.
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),

    /// A `get`-style lookup found no matching row.
    #[error("not found")]
    NotFound,

    /// A host handle used inside a repository (e.g. the audit emitter) failed.
    #[error(transparent)]
    Plugin(#[from] PluginError),
}
