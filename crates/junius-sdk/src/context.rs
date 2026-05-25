//! Per-request plugin context (M07).
//!
//! `PluginContext<S, P>` is what a plugin handler receives: its own state `S`
//! (typically a bundle of repositories), the current caller, the raw
//! `PluginResources`, and a type-level permission witness `P` proving which
//! permissions the request was checked to hold. The `#[derive(PluginCtx)]` macro
//! generates the Axum extractor (and, in Stage D, an RPC entry point) that reads
//! the per-plugin context + caller from request extensions, verifies `P`, and
//! builds the repositories.

use core::marker::PhantomData;

use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};

use crate::auth::User;
use crate::error::RepoError;
use crate::permissions::PermissionList;
use crate::resources::{PluginResourceCtx, PluginResources};

/// Per-request context handed to a plugin handler. `S` is the plugin's state
/// (repositories); `P` is the proven permission witness.
pub struct PluginContext<S, P = ()> {
    /// Plugin state — typically a struct of repositories typed on `P`.
    pub state: S,
    /// The authenticated caller, if any.
    pub user: Option<User>,
    /// Raw host resources (config, telemetry, audit, secrets, users).
    pub resources: PluginResources,
    _phantom: PhantomData<P>,
}

impl<S: Clone, P> Clone for PluginContext<S, P> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            user: self.user.clone(),
            resources: self.resources.clone(),
            _phantom: PhantomData,
        }
    }
}

impl<S, P> PluginContext<S, P> {
    /// Assemble a context. Hidden: called only by `#[derive(PluginCtx)]` output.
    #[doc(hidden)]
    pub fn __new(state: S, user: Option<User>, resources: PluginResources) -> Self {
        Self {
            state,
            user,
            resources,
            _phantom: PhantomData,
        }
    }
}

impl<S, P> PluginContext<S, P>
where
    S: BuildState,
    P: PermissionList,
{
    /// Build the context inside a Connect-RPC handler, from its
    /// [`RequestContext`](connectrpc::RequestContext). Same checks as the HTTP
    /// extractor — resolve resources + caller from request extensions, verify the
    /// witness `P` — but callable from the fixed generated handler signature.
    /// (The host's RPC guard also enforces the proto's declared permissions
    /// independently, before the handler runs.)
    pub fn from_rpc(ctx: &connectrpc::RequestContext) -> Result<Self, ApiError> {
        let (resources, user) = resolve(ctx.extensions())?;
        check_perms(user.as_ref(), &P::names())?;
        let state = S::build(&resources, user.as_ref());
        Ok(Self::__new(state, user, resources))
    }
}

/// Error returned by the `PluginCtx` extractor / RPC entry point: a missing
/// caller (401), a missing permission (403), or unconfigured resources (500).
/// Rendered as a small JSON body matching the Connect error envelope so HTTP and
/// RPC failures look the same on the wire.
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    #[must_use]
    pub fn unauthenticated() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthenticated",
            message: "authentication required".to_string(),
        }
    }

    #[must_use]
    pub fn forbidden(permission: &str) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code: "permission_denied",
            message: format!("missing permission: {permission}"),
        }
    }

    #[must_use]
    pub fn missing_resources() -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal",
            message: "plugin resources not configured for this route".to_string(),
        }
    }

    /// A generic 500 with `message`. Repository failures map here for HTTP
    /// handlers (see `From<RepoError>`); the text is intentionally terse.
    #[must_use]
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal",
            message: message.into(),
        }
    }

    /// The HTTP status this error maps to.
    #[must_use]
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// The Connect error code string.
    #[must_use]
    pub fn code(&self) -> &'static str {
        self.code
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for ApiError {}

impl From<RepoError> for ApiError {
    fn from(err: RepoError) -> Self {
        match err {
            RepoError::NotFound => Self {
                status: StatusCode::NOT_FOUND,
                code: "not_found",
                message: "not found".to_string(),
            },
            other => Self::internal(other.to_string()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = serde_json::json!({ "code": self.code, "message": self.message });
        (self.status, axum::Json(body)).into_response()
    }
}

impl From<ApiError> for connectrpc::ConnectError {
    fn from(err: ApiError) -> Self {
        match err.code {
            "unauthenticated" => Self::unauthenticated(err.message),
            "permission_denied" => Self::permission_denied(err.message),
            "not_found" => Self::not_found(err.message),
            _ => Self::internal(err.message),
        }
    }
}

impl From<RepoError> for connectrpc::ConnectError {
    fn from(err: RepoError) -> Self {
        match err {
            RepoError::NotFound => Self::not_found("not found"),
            other => Self::internal(other.to_string()),
        }
    }
}

impl From<crate::error::PluginError> for connectrpc::ConnectError {
    fn from(err: crate::error::PluginError) -> Self {
        match err {
            crate::error::PluginError::PermissionDenied(msg) => Self::permission_denied(msg),
            other => Self::internal(other.to_string()),
        }
    }
}

/// Build a **privileged, caller-less** context for system work (e.g. background
/// jobs) — no HTTP/RPC caller to authenticate. The witness `P` is chosen by the
/// caller (typically matching the state's `S` parameter): since `BuildState::build`
/// performs no permission check (only `FromRequestParts`/`from_rpc` do), the
/// resulting context runs with the authority of `P`. An under-powered `P` is a
/// compile error at the call site (the repo method isn't nameable), not a runtime
/// bypass. Plugins usually expose a `MyCtx::system(res)` alias over this.
#[must_use]
pub fn system_context<S, P>(resources: &PluginResources) -> PluginContext<S, P>
where
    S: BuildState,
{
    let state = S::build(resources, None);
    PluginContext::__new(state, None, resources.clone())
}

/// How a plugin's state struct builds itself from the per-request resources +
/// caller. `#[derive(PluginCtx)]` implements this for the state (a local type);
/// the blanket `FromRequestParts` impl below — owned here so the orphan rule is
/// satisfied — calls it after resolving resources and checking the witness.
pub trait BuildState: Sized {
    /// Construct the state (its repositories) for this request.
    fn build(resources: &PluginResources, user: Option<&User>) -> Self;
}

/// Resolve the per-plugin context bundle + caller from request extensions.
pub(crate) fn resolve(
    extensions: &axum::http::Extensions,
) -> Result<(PluginResources, Option<User>), ApiError> {
    let ctx = extensions
        .get::<PluginResourceCtx>()
        .ok_or_else(ApiError::missing_resources)?;
    let user = extensions.get::<User>().cloned();
    Ok((PluginResources::from_ctx(ctx, user.clone()), user))
}

/// Verify the caller holds every permission named by the witness, else error.
pub(crate) fn check_perms(user: Option<&User>, names: &[&str]) -> Result<(), ApiError> {
    for &name in names {
        match user {
            Some(u) if u.has_permission(name) => {}
            Some(_) => return Err(ApiError::forbidden(name)),
            None => return Err(ApiError::unauthenticated()),
        }
    }
    Ok(())
}

/// The per-request extractor for every plugin context. Lives here (not in the
/// plugin crate) because `PluginContext` is non-local there — the orphan rule
/// forbids the plugin from implementing the foreign `FromRequestParts` for it.
/// The plugin only implements [`BuildState`] (a local trait) for its own state.
impl<S, P, St> FromRequestParts<St> for PluginContext<S, P>
where
    S: BuildState + Send,
    P: PermissionList + Send + Sync + 'static,
    St: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &St) -> Result<Self, Self::Rejection> {
        let (resources, user) = resolve(&parts.extensions)?;
        check_perms(user.as_ref(), &P::names())?;
        let state = S::build(&resources, user.as_ref());
        Ok(Self::__new(state, user, resources))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests panic on unexpected failures")]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::auth::{GroupId, Membership, Role, RoleId, UserId};

    fn user_with(perms: &[&str]) -> User {
        User {
            id: UserId(uuid::Uuid::nil()),
            email: "u@example.com".into(),
            display_name: "U".into(),
            memberships: vec![Membership {
                group_id: GroupId(uuid::Uuid::nil()),
                group_name: "G".into(),
                role: Role {
                    id: RoleId(uuid::Uuid::nil()),
                    name: "r".into(),
                },
                permissions: perms
                    .iter()
                    .map(|p| (*p).to_string())
                    .collect::<HashSet<_>>(),
            }],
        }
    }

    #[test]
    fn check_perms_ok_when_held() {
        let u = user_with(&["hello:read"]);
        assert!(check_perms(Some(&u), &["hello:read"]).is_ok());
        assert!(check_perms(Some(&u), &[]).is_ok());
    }

    #[test]
    fn check_perms_forbidden_when_missing() {
        let u = user_with(&["hello:read"]);
        let err = check_perms(Some(&u), &["hello:write"]).unwrap_err();
        assert_eq!(err.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn check_perms_unauthenticated_without_user() {
        let err = check_perms(None, &["hello:read"]).unwrap_err();
        assert_eq!(err.status(), StatusCode::UNAUTHORIZED);
        // …but a no-permission witness is fine even without a user.
        assert!(check_perms(None, &[]).is_ok());
    }
}
