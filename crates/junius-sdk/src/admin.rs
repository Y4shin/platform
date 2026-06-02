//! The typed `PlatformAdminApi` accessor — M18's seam for cross-schema writes
//! on `platform.group*` / `platform.user_role*` / `platform.group_membership`.
//!
//! Plugin pools have no SELECT/INSERT on those tables; the `admin` plugin
//! reaches them through this API instead, which runs on the platform pool the
//! host owns. Every mutation emits an audit event (so the admin log is the
//! single source of "who changed which role/permission, when"), and every
//! method is gated at the entry-point on the `platform.admin` capability.
//!
//! The capability is checked **at method entry** (runtime); the host's
//! trusted-capability allowlist additionally ensures *only* the blessed admin
//! plugin can declare it (Stage 2's host-side allowlist + `junius check`
//! rule). The combination keeps the attack surface narrow: even if a
//! third-party plugin attempts to declare `platform.admin`, the host refuses
//! to boot.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::{AuditEmitter, GroupId, RoleId, UserId, UserRoleId};
use crate::error::PluginError;
use crate::resources::require_capability;

/// One permission declared by a plugin in its `[permissions]` manifest block.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPermissionEntry {
    /// The fully-qualified permission key (`<plugin>:<perm>`).
    pub name: String,
    pub description: String,
}

/// The permissions one plugin declares. Aggregated at host boot from each
/// plugin's `PluginMetadata::permissions` slice; published to the admin
/// plugin via `PlatformAdminApi::permission_catalogue`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPermissionsSummary {
    pub plugin: String,
    pub display_name: String,
    pub permissions: Vec<PluginPermissionEntry>,
}

/// A group row as the admin sees it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminGroup {
    pub id: GroupId,
    pub name: String,
    pub description: Option<String>,
}

/// A role within a group, plus the permissions it grants.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminGroupRole {
    pub id: RoleId,
    pub group_id: GroupId,
    pub name: String,
    pub permissions: Vec<String>,
}

/// A group's member, with the provenance discriminator + source for the UI
/// to render "managed by OIDC group X" / "managed by config".
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminMembership {
    pub user_id: UserId,
    pub email: String,
    pub display_name: String,
    pub role_id: RoleId,
    pub role_name: String,
    pub managed_by: String,
    pub managed_source: Option<String>,
}

/// A user-role plus its permission set.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminUserRole {
    pub id: UserRoleId,
    pub name: String,
    pub description: Option<String>,
    pub is_builtin: bool,
    pub permissions: Vec<String>,
}

/// An assignment of a user-role to a user.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminUserRoleAssignment {
    pub user_id: UserId,
    pub email: String,
    pub display_name: String,
    pub role_id: UserRoleId,
    pub role_name: String,
}

/// The cross-schema admin API. Cheap to clone (`Arc`-shared pool + catalogue).
#[derive(Clone)]
pub struct PlatformAdminApi {
    pool: PgPool,
    audit: AuditEmitter,
    plugin_name: &'static str,
    capabilities: &'static [&'static str],
    catalogue: Arc<Vec<PluginPermissionsSummary>>,
    /// Set by [`Self::with_user`] per request; used as `actor_user_id` on
    /// every audit event.
    actor: Option<UserId>,
    /// M18 Stage D — when `true`, mutators that target a row with
    /// `managed_by='config'` refuse with [`PluginError::ManagedByConfig`].
    /// Mirrors `[provisioning] lock_managed` in the deployment TOML.
    lock_managed: bool,
}

impl PlatformAdminApi {
    /// Construct a caller-less handle. The host builds one of these once per
    /// boot and clones it into each plugin's `PluginResourceCtx`; plugins
    /// without the `platform.admin` capability get a copy too but every
    /// method on it refuses (`CapabilityNotDeclared`).
    #[must_use]
    pub fn new(
        pool: PgPool,
        audit: AuditEmitter,
        plugin_name: &'static str,
        capabilities: &'static [&'static str],
        catalogue: Arc<Vec<PluginPermissionsSummary>>,
        lock_managed: bool,
    ) -> Self {
        Self {
            pool,
            audit,
            plugin_name,
            capabilities,
            catalogue,
            actor: None,
            lock_managed,
        }
    }

    /// Attach the current request's caller. The admin RPC guards ensure only
    /// admins call into the API at all; the attached `UserId` is recorded as
    /// `actor_user_id` on each audit event.
    #[must_use]
    pub fn with_user(mut self, user: Option<UserId>) -> Self {
        self.actor = user;
        self
    }

    fn check(&self) -> Result<(), PluginError> {
        require_capability(self.capabilities, "platform.admin")
    }

    /// When `lock_managed` is on, refuse to mutate a `group_membership` row
    /// owned by the deployment's `[provisioning]` block. The only `managed_by`
    /// discriminator in the schema today lives on `group_membership`, so the
    /// lock applies there; rows tagged `'oidc'` are also under reconciler
    /// ownership but the OIDC reaper would re-create or re-delete them on the
    /// next login, so the admin API doesn't block 'oidc' mutations — that
    /// would be the `lock_managed` flag's `oidc` overreach. Only `'config'`.
    async fn ensure_membership_unlocked(
        &self,
        group: GroupId,
        user: UserId,
    ) -> Result<(), PluginError> {
        if !self.lock_managed {
            return Ok(());
        }
        let managed_by: Option<String> = sqlx::query_scalar(
            "SELECT managed_by FROM platform.group_membership \
             WHERE group_id = $1 AND user_id = $2",
        )
        .bind(group.0)
        .bind(user.0)
        .fetch_optional(&self.pool)
        .await?;
        if matches!(managed_by.as_deref(), Some("config")) {
            return Err(PluginError::ManagedByConfig(format!(
                "group_membership(group={}, user={}) is managed by [provisioning]",
                group.0, user.0
            )));
        }
        Ok(())
    }

    async fn audit_event(
        &self,
        kind: &str,
        resource_kind: &str,
        resource_id: Option<Uuid>,
        details: serde_json::Value,
    ) -> Result<(), PluginError> {
        self.audit
            .emit(kind, self.actor, resource_kind, resource_id, details)
            .await
    }

    // ---- Permission catalogue ------------------------------------------------

    /// Every plugin's `[permissions]` block, grouped by plugin. Read-only:
    /// the source is the host's compiled-in `PluginMetadata::permissions`
    /// slice, so it can never drift from what `junius check` accepts.
    pub fn permission_catalogue(&self) -> Result<&[PluginPermissionsSummary], PluginError> {
        self.check()?;
        Ok(&self.catalogue)
    }

    // ---- Groups --------------------------------------------------------------

    pub async fn list_groups(&self) -> Result<Vec<AdminGroup>, PluginError> {
        self.check()?;
        let rows = sqlx::query_as::<_, (Uuid, String, Option<String>)>(
            "SELECT id, name, description FROM platform.\"group\" ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, name, description)| AdminGroup {
                id: GroupId(id),
                name,
                description,
            })
            .collect())
    }

    pub async fn create_group(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> Result<AdminGroup, PluginError> {
        self.check()?;
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO platform.\"group\" (name, description) VALUES ($1, $2) RETURNING id",
        )
        .bind(name)
        .bind(description)
        .fetch_one(&self.pool)
        .await?;
        self.audit_event(
            "admin:group.create",
            "platform:group",
            Some(id),
            serde_json::json!({ "name": name, "description": description }),
        )
        .await?;
        Ok(AdminGroup {
            id: GroupId(id),
            name: name.into(),
            description: description.map(String::from),
        })
    }

    pub async fn update_group(
        &self,
        id: GroupId,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<(), PluginError> {
        self.check()?;
        sqlx::query(
            "UPDATE platform.\"group\" SET \
                 name        = COALESCE($2, name), \
                 description = COALESCE($3, description) \
             WHERE id = $1",
        )
        .bind(id.0)
        .bind(name)
        .bind(description)
        .execute(&self.pool)
        .await?;
        self.audit_event(
            "admin:group.update",
            "platform:group",
            Some(id.0),
            serde_json::json!({ "name": name, "description": description }),
        )
        .await
    }

    pub async fn delete_group(&self, id: GroupId) -> Result<(), PluginError> {
        self.check()?;
        sqlx::query("DELETE FROM platform.\"group\" WHERE id = $1")
            .bind(id.0)
            .execute(&self.pool)
            .await?;
        self.audit_event(
            "admin:group.delete",
            "platform:group",
            Some(id.0),
            serde_json::json!({}),
        )
        .await
    }

    // ---- Group roles ---------------------------------------------------------

    pub async fn list_group_roles(
        &self,
        group: GroupId,
    ) -> Result<Vec<AdminGroupRole>, PluginError> {
        self.check()?;
        let role_rows = sqlx::query_as::<_, (Uuid, String)>(
            "SELECT id, name FROM platform.group_role WHERE group_id = $1 ORDER BY name",
        )
        .bind(group.0)
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::with_capacity(role_rows.len());
        for (rid, name) in role_rows {
            let perms: Vec<String> = sqlx::query_scalar(
                "SELECT permission FROM platform.role_permission \
                 WHERE role_id = $1 ORDER BY permission",
            )
            .bind(rid)
            .fetch_all(&self.pool)
            .await?;
            out.push(AdminGroupRole {
                id: RoleId(rid),
                group_id: group,
                name,
                permissions: perms,
            });
        }
        Ok(out)
    }

    pub async fn create_group_role(
        &self,
        group: GroupId,
        name: &str,
    ) -> Result<AdminGroupRole, PluginError> {
        self.check()?;
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO platform.group_role (group_id, name) VALUES ($1, $2) RETURNING id",
        )
        .bind(group.0)
        .bind(name)
        .fetch_one(&self.pool)
        .await?;
        self.audit_event(
            "admin:role.create",
            "platform:group_role",
            Some(id),
            serde_json::json!({ "group_id": group.0, "name": name }),
        )
        .await?;
        Ok(AdminGroupRole {
            id: RoleId(id),
            group_id: group,
            name: name.into(),
            permissions: vec![],
        })
    }

    pub async fn delete_group_role(&self, role: RoleId) -> Result<(), PluginError> {
        self.check()?;
        sqlx::query("DELETE FROM platform.group_role WHERE id = $1")
            .bind(role.0)
            .execute(&self.pool)
            .await?;
        self.audit_event(
            "admin:role.delete",
            "platform:group_role",
            Some(role.0),
            serde_json::json!({}),
        )
        .await
    }

    /// Replace `role`'s permission set with `perms` in one transaction:
    /// delete the existing rows, insert the new set.
    pub async fn set_group_role_permissions(
        &self,
        role: RoleId,
        perms: &[String],
    ) -> Result<(), PluginError> {
        self.check()?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM platform.role_permission WHERE role_id = $1")
            .bind(role.0)
            .execute(&mut *tx)
            .await?;
        if !perms.is_empty() {
            sqlx::query(
                "INSERT INTO platform.role_permission (role_id, permission) \
                 SELECT $1, p FROM unnest($2::text[]) AS p",
            )
            .bind(role.0)
            .bind(perms)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.audit_event(
            "admin:role.set_permissions",
            "platform:group_role",
            Some(role.0),
            serde_json::json!({ "permissions": perms }),
        )
        .await
    }

    // ---- Group members -------------------------------------------------------

    pub async fn list_group_members(
        &self,
        group: GroupId,
    ) -> Result<Vec<AdminMembership>, PluginError> {
        self.check()?;
        let rows =
            sqlx::query_as::<_, (Uuid, String, String, Uuid, String, String, Option<String>)>(
                "SELECT u.id, u.email, u.display_name, \
                    gm.role_id, gr.name, gm.managed_by, gm.managed_source \
             FROM platform.group_membership gm \
             JOIN platform.\"user\" u ON u.id = gm.user_id \
             JOIN platform.group_role gr ON gr.id = gm.role_id \
             WHERE gm.group_id = $1 \
             ORDER BY u.display_name",
            )
            .bind(group.0)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(
                |(uid, email, dname, rid, rname, managed_by, managed_source)| AdminMembership {
                    user_id: UserId(uid),
                    email,
                    display_name: dname,
                    role_id: RoleId(rid),
                    role_name: rname,
                    managed_by,
                    managed_source,
                },
            )
            .collect())
    }

    /// Add or move a user into `group` with `role`. Always `managed_by =
    /// 'manual'` from this seam — OIDC and config own their own writes
    /// (Stages 3/4) and never go through the admin API. When `lock_managed`
    /// is enabled and the existing row is `managed_by='config'`, the call
    /// refuses with [`PluginError::ManagedByConfig`].
    pub async fn add_group_member(
        &self,
        group: GroupId,
        user: UserId,
        role: RoleId,
    ) -> Result<(), PluginError> {
        self.check()?;
        self.ensure_membership_unlocked(group, user).await?;
        sqlx::query(
            "INSERT INTO platform.group_membership (user_id, group_id, role_id, managed_by) \
             VALUES ($1, $2, $3, 'manual') \
             ON CONFLICT (user_id, group_id) DO UPDATE SET \
                role_id    = EXCLUDED.role_id, \
                managed_by = 'manual', \
                managed_source = NULL",
        )
        .bind(user.0)
        .bind(group.0)
        .bind(role.0)
        .execute(&self.pool)
        .await?;
        self.audit_event(
            "admin:membership.add",
            "platform:group_membership",
            Some(group.0),
            serde_json::json!({
                "group_id": group.0,
                "user_id":  user.0,
                "role_id":  role.0,
            }),
        )
        .await
    }

    pub async fn remove_group_member(
        &self,
        group: GroupId,
        user: UserId,
    ) -> Result<(), PluginError> {
        self.check()?;
        self.ensure_membership_unlocked(group, user).await?;
        sqlx::query("DELETE FROM platform.group_membership WHERE group_id = $1 AND user_id = $2")
            .bind(group.0)
            .bind(user.0)
            .execute(&self.pool)
            .await?;
        self.audit_event(
            "admin:membership.remove",
            "platform:group_membership",
            Some(group.0),
            serde_json::json!({ "group_id": group.0, "user_id": user.0 }),
        )
        .await
    }

    // ---- User-roles ----------------------------------------------------------

    pub async fn list_user_roles(&self) -> Result<Vec<AdminUserRole>, PluginError> {
        self.check()?;
        let role_rows = sqlx::query_as::<_, (Uuid, String, Option<String>, bool)>(
            "SELECT id, name, description, is_builtin \
             FROM platform.user_role ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::with_capacity(role_rows.len());
        for (id, name, description, is_builtin) in role_rows {
            let perms: Vec<String> = sqlx::query_scalar(
                "SELECT permission FROM platform.user_role_permission \
                 WHERE role_id = $1 ORDER BY permission",
            )
            .bind(id)
            .fetch_all(&self.pool)
            .await?;
            out.push(AdminUserRole {
                id: UserRoleId(id),
                name,
                description,
                is_builtin,
                permissions: perms,
            });
        }
        Ok(out)
    }

    pub async fn create_user_role(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> Result<AdminUserRole, PluginError> {
        self.check()?;
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO platform.user_role (name, description, is_builtin) \
             VALUES ($1, $2, false) RETURNING id",
        )
        .bind(name)
        .bind(description)
        .fetch_one(&self.pool)
        .await?;
        self.audit_event(
            "admin:user_role.create",
            "platform:user_role",
            Some(id),
            serde_json::json!({ "name": name, "description": description }),
        )
        .await?;
        Ok(AdminUserRole {
            id: UserRoleId(id),
            name: name.into(),
            description: description.map(String::from),
            is_builtin: false,
            permissions: vec![],
        })
    }

    pub async fn delete_user_role(&self, id: UserRoleId) -> Result<(), PluginError> {
        self.check()?;
        // Builtin roles ('admin') can't be deleted: the constraint is enforced
        // at the API layer with a clear error rather than via a DB trigger so
        // the diagnostic is structured.
        let is_builtin: bool =
            sqlx::query_scalar("SELECT is_builtin FROM platform.user_role WHERE id = $1")
                .bind(id.0)
                .fetch_optional(&self.pool)
                .await?
                .unwrap_or(false);
        if is_builtin {
            return Err(PluginError::PermissionDenied(format!(
                "cannot delete builtin user-role {id}"
            )));
        }
        sqlx::query("DELETE FROM platform.user_role WHERE id = $1")
            .bind(id.0)
            .execute(&self.pool)
            .await?;
        self.audit_event(
            "admin:user_role.delete",
            "platform:user_role",
            Some(id.0),
            serde_json::json!({}),
        )
        .await
    }

    /// Replace `role`'s permission set with `perms`. The wildcard `'*'` is
    /// permitted (that's how custom super-admins are configured) but always
    /// audited explicitly.
    pub async fn set_user_role_permissions(
        &self,
        role: UserRoleId,
        perms: &[String],
    ) -> Result<(), PluginError> {
        self.check()?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM platform.user_role_permission WHERE role_id = $1")
            .bind(role.0)
            .execute(&mut *tx)
            .await?;
        if !perms.is_empty() {
            sqlx::query(
                "INSERT INTO platform.user_role_permission (role_id, permission) \
                 SELECT $1, p FROM unnest($2::text[]) AS p",
            )
            .bind(role.0)
            .bind(perms)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.audit_event(
            "admin:user_role.set_permissions",
            "platform:user_role",
            Some(role.0),
            serde_json::json!({ "permissions": perms }),
        )
        .await
    }

    // ---- User-role assignments ----------------------------------------------

    pub async fn list_user_role_assignments(
        &self,
    ) -> Result<Vec<AdminUserRoleAssignment>, PluginError> {
        self.check()?;
        let rows = sqlx::query_as::<_, (Uuid, String, String, Uuid, String)>(
            "SELECT u.id, u.email, u.display_name, ur.id, ur.name \
             FROM platform.user_role_assignment ura \
             JOIN platform.\"user\" u ON u.id = ura.user_id \
             JOIN platform.user_role ur ON ur.id = ura.role_id \
             ORDER BY ur.name, u.display_name",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(uid, email, dname, rid, rname)| AdminUserRoleAssignment {
                user_id: UserId(uid),
                email,
                display_name: dname,
                role_id: UserRoleId(rid),
                role_name: rname,
            })
            .collect())
    }

    pub async fn assign_user_role(
        &self,
        user: UserId,
        role: UserRoleId,
    ) -> Result<(), PluginError> {
        self.check()?;
        sqlx::query(
            "INSERT INTO platform.user_role_assignment (user_id, role_id, granted_by) \
             VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(user.0)
        .bind(role.0)
        .bind(self.actor.map(|u| u.0))
        .execute(&self.pool)
        .await?;
        self.audit_event(
            "admin:user_role.assign",
            "platform:user_role_assignment",
            Some(role.0),
            serde_json::json!({ "user_id": user.0, "role_id": role.0 }),
        )
        .await
    }

    pub async fn revoke_user_role(
        &self,
        user: UserId,
        role: UserRoleId,
    ) -> Result<(), PluginError> {
        self.check()?;
        sqlx::query(
            "DELETE FROM platform.user_role_assignment WHERE user_id = $1 AND role_id = $2",
        )
        .bind(user.0)
        .bind(role.0)
        .execute(&self.pool)
        .await?;
        self.audit_event(
            "admin:user_role.revoke",
            "platform:user_role_assignment",
            Some(role.0),
            serde_json::json!({ "user_id": user.0, "role_id": role.0 }),
        )
        .await
    }

    /// Lookup a user by email — handy for the admin UI's "add member" flow
    /// where the operator only knows the user's email, not their UUID.
    pub async fn find_user_by_email(&self, email: &str) -> Result<Option<UserId>, PluginError> {
        self.check()?;
        let id: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM platform.\"user\" WHERE email = $1")
                .bind(email)
                .fetch_optional(&self.pool)
                .await?;
        Ok(id.map(UserId))
    }

    /// The plugin name the API was built for; used in error messages.
    #[must_use]
    pub fn plugin_name(&self) -> &'static str {
        self.plugin_name
    }

    // ---- Audit log (M19 Slice 6) ---------------------------------------------

    /// List audit events with cursor-based keyset pagination and optional
    /// filters. Results are newest-first by `(occurred_at DESC, id DESC)`.
    pub async fn list_audit_events(
        &self,
        filter: &AuditFilter<'_>,
        cursor: Option<&AuditCursor>,
        limit: i32,
    ) -> Result<AuditPage, PluginError> {
        self.check()?;

        let limit = limit.clamp(1, 200);
        let fetch_limit = i64::from(limit) + 1;

        let rows = sqlx::query_as::<
            _,
            (
                Uuid,
                String,
                Option<Uuid>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<Uuid>,
                serde_json::Value,
                DateTime<Utc>,
            ),
        >(
            "SELECT ae.id, ae.event_kind, ae.actor_user_id, \
                    u.email, u.display_name, \
                    ae.resource_kind, ae.resource_id, ae.details, ae.occurred_at \
             FROM platform.audit_event ae \
             LEFT JOIN platform.\"user\" u ON u.id = ae.actor_user_id \
             WHERE ($1::uuid IS NULL OR ae.actor_user_id = $1) \
               AND ($2::text IS NULL OR ae.resource_kind = $2) \
               AND ($3::text IS NULL OR ae.event_kind = $3) \
               AND ($4::timestamptz IS NULL OR ae.occurred_at >= $4) \
               AND ($5::timestamptz IS NULL OR ae.occurred_at <= $5) \
               AND (($6::timestamptz IS NULL AND $7::uuid IS NULL) \
                    OR (ae.occurred_at, ae.id) < ($6, $7)) \
             ORDER BY ae.occurred_at DESC, ae.id DESC \
             LIMIT $8",
        )
        .bind(filter.actor_user_id)
        .bind(filter.resource_kind)
        .bind(filter.event_kind)
        .bind(filter.starts_at)
        .bind(filter.ends_at)
        .bind(cursor.map(|c| c.occurred_at))
        .bind(cursor.map(|c| c.id))
        .bind(fetch_limit)
        .fetch_all(&self.pool)
        .await?;

        let limit_usize = usize::try_from(limit).unwrap_or(0);
        let has_more = rows.len() > limit_usize;
        let events: Vec<AdminAuditEvent> = rows
            .into_iter()
            .take(limit_usize)
            .map(
                |(id, event_kind, actor_uid, email, dname, rk, rid, details, occurred_at)| {
                    AdminAuditEvent {
                        id,
                        event_kind,
                        actor_user_id: actor_uid.map(UserId),
                        actor_email: email,
                        actor_display_name: dname,
                        resource_kind: rk,
                        resource_id: rid,
                        details,
                        occurred_at,
                    }
                },
            )
            .collect();
        let next_cursor = if has_more {
            events.last().map(|e| AuditCursor {
                occurred_at: e.occurred_at,
                id: e.id,
            })
        } else {
            None
        };
        Ok(AuditPage {
            events,
            next_cursor,
        })
    }

    // ---- OIDC group mappings (M18 Stage C) ----------------------------------

    pub async fn list_oidc_mappings(&self) -> Result<Vec<AdminOidcMapping>, PluginError> {
        self.check()?;
        let rows = sqlx::query_as::<_, (Uuid, String, Uuid, String, Uuid, String)>(
            "SELECT m.id, m.oidc_group_name, \
                    g.id, g.name, \
                    r.id, r.name \
             FROM platform.oidc_group_mapping m \
             JOIN platform.\"group\" g ON g.id = m.group_id \
             JOIN platform.group_role r ON r.id = m.role_id \
             ORDER BY m.oidc_group_name, g.name",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, oidc_group_name, gid, gname, rid, rname)| AdminOidcMapping {
                    id,
                    oidc_group_name,
                    group_id: GroupId(gid),
                    group_name: gname,
                    role_id: RoleId(rid),
                    role_name: rname,
                },
            )
            .collect())
    }

    pub async fn create_oidc_mapping(
        &self,
        oidc_group_name: &str,
        group: GroupId,
        role: RoleId,
    ) -> Result<Uuid, PluginError> {
        self.check()?;
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO platform.oidc_group_mapping (oidc_group_name, group_id, role_id) \
             VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(oidc_group_name)
        .bind(group.0)
        .bind(role.0)
        .fetch_one(&self.pool)
        .await?;
        self.audit_event(
            "admin:oidc_mapping.create",
            "platform:oidc_group_mapping",
            Some(id),
            serde_json::json!({
                "oidc_group_name": oidc_group_name,
                "group_id": group.0,
                "role_id":  role.0,
            }),
        )
        .await?;
        Ok(id)
    }

    pub async fn delete_oidc_mapping(&self, id: Uuid) -> Result<(), PluginError> {
        self.check()?;
        sqlx::query("DELETE FROM platform.oidc_group_mapping WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        self.audit_event(
            "admin:oidc_mapping.delete",
            "platform:oidc_group_mapping",
            Some(id),
            serde_json::json!({}),
        )
        .await
    }
}

/// An OIDC group → Junius group/role mapping, joined with names for the UI.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminOidcMapping {
    pub id: Uuid,
    pub oidc_group_name: String,
    pub group_id: GroupId,
    pub group_name: String,
    pub role_id: RoleId,
    pub role_name: String,
}

/// A row from `platform.audit_event`, joined with actor user info.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminAuditEvent {
    pub id: Uuid,
    pub event_kind: String,
    pub actor_user_id: Option<UserId>,
    pub actor_email: Option<String>,
    pub actor_display_name: Option<String>,
    pub resource_kind: Option<String>,
    pub resource_id: Option<Uuid>,
    pub details: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
}

/// Filter parameters for listing audit events.
pub struct AuditFilter<'a> {
    pub actor_user_id: Option<Uuid>,
    pub resource_kind: Option<&'a str>,
    pub event_kind: Option<&'a str>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
}

/// Opaque cursor for keyset pagination over audit events.
#[derive(Clone, Debug)]
pub struct AuditCursor {
    pub occurred_at: DateTime<Utc>,
    pub id: Uuid,
}

/// A page of audit events with an optional next-page cursor.
#[derive(Clone, Debug)]
pub struct AuditPage {
    pub events: Vec<AdminAuditEvent>,
    pub next_cursor: Option<AuditCursor>,
}
