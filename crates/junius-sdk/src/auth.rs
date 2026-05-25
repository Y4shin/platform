//! Identity types and host-provided auth handles.
//!
//! The host owns sessions, users, groups, roles, and access checks (see
//! `docs/design/10-infrastructure-and-data.md` §10.7). Plugins consume them
//! through these handles on `PluginResources`:
//! - [`Auth`] — the current request's caller plus user-directory lookups.
//! - [`Users`] — a user-directory handle (display names without joining
//!   `platform.user` directly).
//! - [`AuditEmitter`] — append-only writes to `platform.audit_event`.
//!
//! The *current caller* is request-scoped, so it is delivered via the
//! [`CurrentUser`] / [`MaybeUser`] extractors (or `resources.auth.current_user()`),
//! not as construction-time state.

use std::collections::HashSet;

use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::error::PluginError;

macro_rules! id_newtype {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

id_newtype!(
    /// A `platform.user` id.
    UserId
);
id_newtype!(
    /// A `platform.group` id.
    GroupId
);
id_newtype!(
    /// A `platform.group_role` id.
    RoleId
);

/// A role within a group.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Role {
    pub id: RoleId,
    pub name: String,
}

/// A user's membership of a single group, with the permissions their role grants.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Membership {
    pub group_id: GroupId,
    pub group_name: String,
    pub role: Role,
    pub permissions: HashSet<String>,
}

/// The authenticated user, as returned by `/api/me` and carried in request
/// extensions by the host's session middleware. Serialized camelCase to match
/// the frontend `User` type.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: UserId,
    pub email: String,
    pub display_name: String,
    pub memberships: Vec<Membership>,
}

impl User {
    /// Whether the user holds `permission` (e.g. `"hello:read"`) through any of
    /// their group memberships. The host's session middleware populates
    /// `memberships[].permissions`; the `PluginCtx` extractor and the RPC guard
    /// use this to enforce a handler's declared permission witness at runtime.
    #[must_use]
    pub fn has_permission(&self, permission: &str) -> bool {
        self.memberships
            .iter()
            .any(|m| m.permissions.contains(permission))
    }
}

/// Minimal public projection of a user, for directory lookups.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserDisplay {
    pub id: UserId,
    pub email: String,
    pub display_name: String,
}

/// Caller identity + user-directory access. Built per request: the base handle
/// carries a `platform`-scoped pool; the session middleware attaches the
/// current user via [`Auth::with_user`].
#[derive(Clone)]
pub struct Auth {
    pool: PgPool,
    current: Option<User>,
}

impl Auth {
    /// Construct a caller-less handle over the platform pool.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            current: None,
        }
    }

    /// Attach the current request's user.
    #[must_use]
    pub fn with_user(mut self, user: Option<User>) -> Self {
        self.current = user;
        self
    }

    /// The authenticated caller for this request, if any.
    #[must_use]
    pub fn current_user(&self) -> Option<&User> {
        self.current.as_ref()
    }

    /// Look up any user's public projection by id.
    pub async fn lookup_user(&self, id: UserId) -> Result<Option<UserDisplay>, PluginError> {
        lookup_display(&self.pool, id).await
    }
}

/// A user-directory handle: resolve user ids to display info without touching
/// `platform.user` directly.
#[derive(Clone)]
pub struct Users {
    pool: PgPool,
}

impl Users {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn lookup(&self, id: UserId) -> Result<Option<UserDisplay>, PluginError> {
        lookup_display(&self.pool, id).await
    }
}

async fn lookup_display(pool: &PgPool, id: UserId) -> Result<Option<UserDisplay>, PluginError> {
    let row = sqlx::query("SELECT id, email, display_name FROM platform.user WHERE id = $1")
        .bind(id.0)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| UserDisplay {
        id: UserId(r.get("id")),
        email: r.get("email"),
        display_name: r.get("display_name"),
    }))
}

/// Minimal public projection of a group, for directory lookups.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupRef {
    pub id: GroupId,
    pub name: String,
    pub description: Option<String>,
}

/// A member of a group, with the role they hold there.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupMember {
    pub user: UserDisplay,
    pub role: Role,
}

/// A group-directory handle: resolve groups by name/id and enumerate their
/// members without touching `platform.group*` directly. Like [`Users`], it runs
/// on the platform pool, so it needs no per-plugin grant.
#[derive(Clone)]
pub struct Groups {
    pool: PgPool,
}

impl Groups {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Resolve a group by its platform `name`. Group names are **not** unique in
    /// `platform.group`; this returns the earliest-created match (callers that
    /// need determinism should prefer ids — e.g. the calendar feed keeps an
    /// id-based path as canonical).
    pub async fn by_name(&self, name: &str) -> Result<Option<GroupRef>, PluginError> {
        let row = sqlx::query(
            "SELECT id, name, description FROM platform.group \
             WHERE name = $1 ORDER BY created_at LIMIT 1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| GroupRef {
            id: GroupId(r.get("id")),
            name: r.get("name"),
            description: r.get("description"),
        }))
    }

    /// Look up a group by id.
    pub async fn by_id(&self, id: GroupId) -> Result<Option<GroupRef>, PluginError> {
        let row = sqlx::query("SELECT id, name, description FROM platform.group WHERE id = $1")
            .bind(id.0)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| GroupRef {
            id: GroupId(r.get("id")),
            name: r.get("name"),
            description: r.get("description"),
        }))
    }

    /// Whether `user` is a member of `group`.
    pub async fn is_member(&self, group: GroupId, user: UserId) -> Result<bool, PluginError> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM platform.group_membership \
             WHERE group_id = $1 AND user_id = $2)",
        )
        .bind(group.0)
        .bind(user.0)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists)
    }

    /// Enumerate a group's members (e.g. for group sign-up pre-fill),
    /// **authorization-checked**: `as_caller` must themselves be a member of
    /// `group`, otherwise this returns [`PluginError::PermissionDenied`] without
    /// disclosing the roster.
    ///
    /// Unlike [`by_name`](Self::by_name)/[`by_id`](Self::by_id) (open directory
    /// lookups that return only a group's name/description, mirroring
    /// [`Users::lookup`]), `members` exposes every member's email — the sensitive
    /// directory path — so it is gated on the caller's own membership rather than
    /// left open on the platform pool.
    pub async fn members(
        &self,
        group: GroupId,
        as_caller: UserId,
    ) -> Result<Vec<GroupMember>, PluginError> {
        if !self.is_member(group, as_caller).await? {
            return Err(PluginError::PermissionDenied(format!(
                "user {as_caller} is not a member of group {group}"
            )));
        }
        let rows = sqlx::query(
            "SELECT u.id, u.email, u.display_name, r.id AS role_id, r.name AS role_name \
             FROM platform.group_membership m \
             JOIN platform.user u ON u.id = m.user_id \
             JOIN platform.group_role r ON r.id = m.role_id \
             WHERE m.group_id = $1 ORDER BY u.display_name",
        )
        .bind(group.0)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| GroupMember {
                user: UserDisplay {
                    id: UserId(r.get("id")),
                    email: r.get("email"),
                    display_name: r.get("display_name"),
                },
                role: Role {
                    id: RoleId(r.get("role_id")),
                    name: r.get("role_name"),
                },
            })
            .collect())
    }
}

/// Append-only audit logging into `platform.audit_event`.
#[derive(Clone)]
pub struct AuditEmitter {
    pool: PgPool,
}

impl AuditEmitter {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Record an audit event. `event_kind` is `<plugin>:<resource>.<verb>`;
    /// `resource_kind` is `<plugin>:<table>`.
    pub async fn emit(
        &self,
        event_kind: &str,
        actor_user_id: Option<UserId>,
        resource_kind: &str,
        resource_id: Option<Uuid>,
        details: serde_json::Value,
    ) -> Result<(), PluginError> {
        sqlx::query(
            "INSERT INTO platform.audit_event \
             (event_kind, actor_user_id, resource_kind, resource_id, details) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(event_kind)
        .bind(actor_user_id.map(|u| u.0))
        .bind(resource_kind)
        .bind(resource_id)
        .bind(details)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

/// Extractor for the authenticated caller. Rejects with `401` when the request
/// has no session-resolved user.
pub struct CurrentUser(pub User);

impl<S: Send + Sync> FromRequestParts<S> for CurrentUser {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<User>()
            .cloned()
            .map(CurrentUser)
            .ok_or_else(|| (StatusCode::UNAUTHORIZED, "authentication required").into_response())
    }
}

/// Extractor for the optional authenticated caller; never rejects.
pub struct MaybeUser(pub Option<User>);

impl<S: Send + Sync> FromRequestParts<S> for MaybeUser {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(MaybeUser(parts.extensions.get::<User>().cloned()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests panic on unexpected failures")]
mod tests {
    use super::*;

    #[test]
    fn user_id_serde_is_transparent() {
        let id = UserId(Uuid::nil());
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"00000000-0000-0000-0000-000000000000\"");
    }
}
