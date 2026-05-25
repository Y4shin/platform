//! `PluginResources` — the bundle of host-provided handles a plugin uses.
//!
//! From M06, `PluginResources` is **request-scoped**: it is produced per request
//! via the [`FromRequestParts`] extractor (so handlers get the DB handle *and*
//! the current caller in one value), and also built once at boot for the
//! `on_startup`/`on_shutdown` lifecycle hooks (with no caller).
//!
//! The request-independent parts (config, telemetry, db pool, directory, audit)
//! are assembled once by the host into a [`PluginResourceCtx`] and attached to
//! each plugin's router subtree as an `Extension`. The extractor combines that
//! with the session-resolved [`User`] in request extensions.

use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};

use crate::auth::{AuditEmitter, Auth, User, Users};
use crate::authz::Authz;
use crate::config::PluginConfig;
use crate::db::PluginDb;
use crate::secrets::SecretStore;
use crate::telemetry::Telemetry;

/// Everything a plugin handler is handed for the current request.
#[derive(Clone)]
pub struct PluginResources {
    pub config: PluginConfig,
    pub telemetry: Telemetry,
    /// Opaque DB handle; the query API arrives in M07.
    pub(crate) db: PluginDb,
    /// Caller identity + user-directory lookups (caller filled per request).
    pub auth: Auth,
    /// User-directory handle.
    pub users: Users,
    /// Audit-log writer.
    pub audit: AuditEmitter,
    /// Per-resource ownership + sharing (caller attached per request).
    pub authz: Authz,
    /// Resolved secrets; read via the codegen'd `Secrets` accessor.
    pub(crate) secrets: SecretStore,
}

impl PluginResources {
    /// Build a request-scoped `PluginResources` from the per-plugin context and
    /// the (optional) current caller.
    #[must_use]
    pub fn from_ctx(ctx: &PluginResourceCtx, user: Option<User>) -> Self {
        Self {
            config: ctx.config.clone(),
            telemetry: ctx.telemetry.clone(),
            db: ctx.db.clone(),
            auth: ctx.auth.clone().with_user(user.clone()),
            users: ctx.users.clone(),
            audit: ctx.audit.clone(),
            authz: ctx.authz.clone().with_user(user.map(|u| u.id)),
            secrets: ctx.secrets.clone(),
        }
    }

    /// The plugin's opaque DB handle (M07 unlocks queries through it).
    #[must_use]
    pub fn db(&self) -> &PluginDb {
        &self.db
    }

    /// The plugin's resolved secrets. The codegen'd `Secrets::new(...)` wraps
    /// this to expose declared secrets by name.
    #[must_use]
    pub fn secrets(&self) -> &SecretStore {
        &self.secrets
    }
}

/// Request-independent per-plugin handles, assembled once by the host and
/// attached to the plugin's router as an `Extension`. The `auth` handle here is
/// caller-less; the extractor attaches the current user.
#[derive(Clone)]
pub struct PluginResourceCtx {
    config: PluginConfig,
    telemetry: Telemetry,
    db: PluginDb,
    auth: Auth,
    users: Users,
    audit: AuditEmitter,
    authz: Authz,
    secrets: SecretStore,
}

impl PluginResourceCtx {
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        reason = "one constructor arg per host-provided handle; assembled once at boot"
    )]
    pub fn new(
        config: PluginConfig,
        telemetry: Telemetry,
        db: PluginDb,
        auth: Auth,
        users: Users,
        audit: AuditEmitter,
        authz: Authz,
        secrets: SecretStore,
    ) -> Self {
        Self {
            config,
            telemetry,
            db,
            auth,
            users,
            audit,
            authz,
            secrets,
        }
    }
}

impl<S: Send + Sync> FromRequestParts<S> for PluginResources {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let ctx = parts.extensions.get::<PluginResourceCtx>().ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "plugin resources not configured for this route",
            )
                .into_response()
        })?;
        let user = parts.extensions.get::<User>().cloned();
        Ok(PluginResources::from_ctx(ctx, user))
    }
}
