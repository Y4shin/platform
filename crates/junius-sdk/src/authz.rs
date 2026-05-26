//! Per-resource access control (M08).
//!
//! [`Authz`] is the host handle plugins use to record ownership of resources they
//! create and to share them. It connects as the host's `platform` role (the
//! same pool as [`Auth`](crate::auth::Auth)/[`AuditEmitter`]) because the
//! `platform.resource_principal`/`resource_share` tables aren't writable by a
//! plugin's least-privilege role.
//!
//! Reads (`platform.user_can_access`) and ownership recording
//! (`platform.record_owner`) are `SECURITY DEFINER` SQL functions, so a plugin's
//! repository can call them through its own role/transaction — recording
//! ownership stays atomic with the resource INSERT.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::auth::AuditEmitter;
use crate::auth::{GroupId, UserId};
use crate::error::PluginError;

/// Who a resource is owned by or shared with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Principal {
    User(UserId),
    Group(GroupId),
    /// Everyone (shares only — a resource can't be *owned* by the public).
    Public,
}

impl Principal {
    /// Decompose into the `(principal_kind, user_id, group_id)` columns.
    fn columns(self) -> (&'static str, Option<Uuid>, Option<Uuid>) {
        match self {
            Self::User(id) => ("user", Some(id.0), None),
            Self::Group(id) => ("group", None, Some(id.0)),
            Self::Public => ("public", None, None),
        }
    }
}

/// A recorded share.
#[derive(Debug, Clone, Copy)]
pub struct ShareRecord {
    pub id: Uuid,
}

/// Host handle for ownership + sharing. Cheap to clone (the pool is `Arc` inside).
#[derive(Clone)]
pub struct Authz {
    pool: PgPool,
    audit: AuditEmitter,
    current: Option<UserId>,
}

impl Authz {
    /// Build the caller-less handle (the per-request extractor attaches the user).
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self {
            audit: AuditEmitter::new(pool.clone()),
            pool,
            current: None,
        }
    }

    /// Attach the current caller (used for owner checks + audit attribution).
    #[must_use]
    pub fn with_user(mut self, user: Option<UserId>) -> Self {
        self.current = user;
        self
    }

    /// Record ownership of a freshly-created resource, inside the *plugin's* own
    /// transaction (atomic with the resource INSERT). The privileged write
    /// happens in the `platform.record_owner` definer function.
    pub async fn record_owner(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        resource_kind: &str,
        resource_id: Uuid,
        owner: Principal,
    ) -> Result<(), PluginError> {
        let (owner_user, owner_group) = match owner {
            Principal::User(id) => (Some(id.0), None),
            Principal::Group(id) => (None, Some(id.0)),
            Principal::Public => {
                return Err(PluginError::PermissionDenied(
                    "a resource cannot be owned by the public".to_string(),
                ));
            }
        };
        sqlx::query("SELECT platform.record_owner($1, $2, $3, $4)")
            .bind(resource_kind)
            .bind(resource_id)
            .bind(owner_user)
            .bind(owner_group)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    /// Delete-time counterpart to [`record_owner`]: clear a resource's
    /// `resource_principal` + `resource_share` rows inside the plugin's own
    /// transaction (atomic with the row delete), so deleting an owned resource
    /// leaves no orphaned ACL rows. Plugin roles can't DELETE the host tables
    /// directly; the privileged work happens in `platform.forget_resource`.
    ///
    /// No-op if the resource was never recorded — the SQL DELETEs match zero
    /// rows. Callers don't need to check existence first.
    pub async fn forget_resource(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        resource_kind: &str,
        resource_id: Uuid,
    ) -> Result<(), PluginError> {
        sqlx::query("SELECT platform.forget_resource($1, $2)")
            .bind(resource_kind)
            .bind(resource_id)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    /// Grant `permission` on a resource to `principal`. Only the resource's owner
    /// (or a member of the owning group) may share; emits an audit event.
    pub async fn share(
        &self,
        resource_kind: &str,
        resource_id: Uuid,
        principal: Principal,
        permission: &str,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<ShareRecord, PluginError> {
        let caller = self.require_owner(resource_kind, resource_id).await?;
        let (principal_kind, principal_user, principal_group) = principal.columns();

        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO platform.resource_share \
             (resource_kind, resource_id, principal_kind, principal_user_id, \
              principal_group_id, permission, granted_by_user_id, expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
        )
        .bind(resource_kind)
        .bind(resource_id)
        .bind(principal_kind)
        .bind(principal_user)
        .bind(principal_group)
        .bind(permission)
        .bind(caller.0)
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await?;

        self.audit
            .emit(
                &format!("{resource_kind}.share"),
                Some(caller),
                resource_kind,
                Some(resource_id),
                serde_json::json!({ "permission": permission, "principal_kind": principal_kind }),
            )
            .await?;

        Ok(ShareRecord { id })
    }

    /// Revoke a share by id. Only the owner may unshare; emits an audit event.
    pub async fn unshare(
        &self,
        resource_kind: &str,
        resource_id: Uuid,
        share_id: Uuid,
    ) -> Result<(), PluginError> {
        let caller = self.require_owner(resource_kind, resource_id).await?;
        sqlx::query(
            "DELETE FROM platform.resource_share \
             WHERE id = $1 AND resource_kind = $2 AND resource_id = $3",
        )
        .bind(share_id)
        .bind(resource_kind)
        .bind(resource_id)
        .execute(&self.pool)
        .await?;

        self.audit
            .emit(
                &format!("{resource_kind}.unshare"),
                Some(caller),
                resource_kind,
                Some(resource_id),
                serde_json::json!({ "share_id": share_id }),
            )
            .await?;
        Ok(())
    }

    /// Resolve the caller and confirm they own the resource (directly, or as a
    /// member of the owning group). Returns the caller id on success.
    async fn require_owner(
        &self,
        resource_kind: &str,
        resource_id: Uuid,
    ) -> Result<UserId, PluginError> {
        let caller = self
            .current
            .ok_or_else(|| PluginError::PermissionDenied("authentication required".to_string()))?;

        let is_owner: Option<bool> = sqlx::query_scalar(
            "SELECT CASE \
               WHEN owner_user_id = $3 THEN true \
               WHEN owner_group_id IS NOT NULL AND EXISTS ( \
                 SELECT 1 FROM platform.group_membership \
                 WHERE user_id = $3 AND group_id = owner_group_id) THEN true \
               ELSE false END \
             FROM platform.resource_principal \
             WHERE resource_kind = $1 AND resource_id = $2",
        )
        .bind(resource_kind)
        .bind(resource_id)
        .bind(caller.0)
        .fetch_optional(&self.pool)
        .await?;

        if is_owner == Some(true) {
            Ok(caller)
        } else {
            Err(PluginError::PermissionDenied(format!(
                "only the owner may share {resource_kind}"
            )))
        }
    }
}
