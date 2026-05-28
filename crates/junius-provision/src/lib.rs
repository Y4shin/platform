//! Shared DB reconcile logic for the `[provisioning]` block (M18 Stage D).
//!
//! The `junius provision apply` CLI and the host's boot-time `auto_apply_on_boot`
//! pass both use this crate: a single [`apply`] entry point that consults the
//! `platform.provisioning_state` hash, reconciles when needed, and persists the
//! new hash on success. The CLI adds `--force` (re-apply regardless of hash);
//! the host calls [`apply`] with `force = false` so an unchanged config is one
//! comparison plus zero writes.
//!
//! What's reconciled today:
//! - Groups (by name) — upserted, description updated when changed.
//! - Group roles (by `(group, role_name)`) — upserted, permission set
//!   replaced.
//! - User-roles (by name) — upserted, permission set replaced. Built-in rows
//!   preserve `is_builtin = true`.
//! - User-role assignments (by `oidc_sub` or `email`) — upserted. A user
//!   referenced by email that has never logged in is skipped with a warning.
//! - OIDC group mappings (by `(oidc_group_name, group_id)`) — upserted.
//!
//! What's **not yet** done:
//! - Destructive reconciliation (drop a `managed_by='config'` row whose
//!   declaration disappeared from the file). `--allow-delete` is parsed
//!   at the CLI but a no-op in this iteration.

use junius_manifest::{OidcMappingDecl, ProvisioningConfig, UserRoleAssignmentDecl, UserRoleDecl};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ProvisionError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
}

/// One run's summary, returned to the caller for logging.
#[derive(Debug, Clone)]
pub struct ApplyOutcome {
    /// Hash of the resolved config that was (or would have been) applied.
    pub hash: String,
    /// `true` when the hash matched and reconcile was skipped.
    pub unchanged: bool,
    /// Number of declarations of each kind in the applied config.
    pub groups: usize,
    pub user_roles: usize,
    pub user_role_assignments: usize,
    pub oidc_mappings: usize,
}

/// Reconcile the DB against `config`. When `force = false` and the stored
/// hash in `platform.provisioning_state` matches the config's content hash,
/// the reconcile is skipped and [`ApplyOutcome::unchanged`] is `true`.
pub async fn apply(
    pool: &PgPool,
    config: &ProvisioningConfig,
    force: bool,
) -> Result<ApplyOutcome, ProvisionError> {
    let hash = config.content_hash();
    let summary = ApplyOutcome {
        hash: hash.clone(),
        unchanged: false,
        groups: config.groups.len(),
        user_roles: config.user_roles.len(),
        user_role_assignments: config.user_role_assignments.len(),
        oidc_mappings: config.oidc_mappings.len(),
    };

    if !force {
        let current = current_hash(pool).await?;
        if current.as_deref() == Some(hash.as_str()) {
            return Ok(ApplyOutcome {
                unchanged: true,
                ..summary
            });
        }
    }

    reconcile(pool, config).await?;
    persist_hash(pool, &hash).await?;
    Ok(summary)
}

/// Read the last-applied hash from `platform.provisioning_state`.
pub async fn current_hash(pool: &PgPool) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT last_hash FROM platform.provisioning_state WHERE id = 1")
        .fetch_optional(pool)
        .await
}

async fn persist_hash(pool: &PgPool, hash: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO platform.provisioning_state (id, last_hash, applied_at) \
         VALUES (1, $1, now()) \
         ON CONFLICT (id) DO UPDATE SET last_hash = EXCLUDED.last_hash, applied_at = now()",
    )
    .bind(hash)
    .execute(pool)
    .await?;
    Ok(())
}

async fn reconcile(pool: &PgPool, p: &ProvisioningConfig) -> Result<(), sqlx::Error> {
    for g in &p.groups {
        let group_id = upsert_group(pool, &g.name, g.description.as_deref()).await?;
        for r in &g.roles {
            let role_id = upsert_group_role(pool, group_id, &r.name).await?;
            set_group_role_permissions(pool, role_id, &r.permissions).await?;
        }
    }
    for ur in &p.user_roles {
        upsert_user_role(pool, ur).await?;
    }
    for a in &p.user_role_assignments {
        if let Err(e) = apply_user_role_assignment(pool, a).await {
            tracing::warn!(
                user = a.oidc_sub.as_deref().or(a.email.as_deref()).unwrap_or("?"),
                role = %a.user_role,
                error = %e,
                "skipping user_role_assignment",
            );
        }
    }
    for m in &p.oidc_mappings {
        if let Err(e) = apply_oidc_mapping(pool, m).await {
            tracing::warn!(
                oidc_group = %m.oidc_group,
                group = %m.group,
                role = %m.role,
                error = %e,
                "skipping oidc_mapping",
            );
        }
    }
    Ok(())
}

async fn upsert_group(
    pool: &PgPool,
    name: &str,
    description: Option<&str>,
) -> Result<Uuid, sqlx::Error> {
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM platform.\"group\" WHERE name = $1 ORDER BY created_at LIMIT 1",
    )
    .bind(name)
    .fetch_optional(pool)
    .await?;
    if let Some(id) = existing {
        sqlx::query("UPDATE platform.\"group\" SET description = $1 WHERE id = $2")
            .bind(description)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(id)
    } else {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO platform.\"group\" (name, description) VALUES ($1, $2) RETURNING id",
        )
        .bind(name)
        .bind(description)
        .fetch_one(pool)
        .await?;
        Ok(id)
    }
}

async fn upsert_group_role(pool: &PgPool, group_id: Uuid, name: &str) -> Result<Uuid, sqlx::Error> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO platform.group_role (group_id, name) \
         VALUES ($1, $2) \
         ON CONFLICT (group_id, name) DO UPDATE SET name = EXCLUDED.name \
         RETURNING id",
    )
    .bind(group_id)
    .bind(name)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

async fn set_group_role_permissions(
    pool: &PgPool,
    role_id: Uuid,
    perms: &[String],
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM platform.role_permission WHERE role_id = $1")
        .bind(role_id)
        .execute(&mut *tx)
        .await?;
    if !perms.is_empty() {
        sqlx::query(
            "INSERT INTO platform.role_permission (role_id, permission) \
             SELECT $1, p FROM unnest($2::text[]) AS p",
        )
        .bind(role_id)
        .bind(perms)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

async fn upsert_user_role(pool: &PgPool, ur: &UserRoleDecl) -> Result<(), sqlx::Error> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO platform.user_role (name, description, is_builtin) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (name) DO UPDATE SET \
             description = EXCLUDED.description, \
             is_builtin  = platform.user_role.is_builtin OR EXCLUDED.is_builtin \
         RETURNING id",
    )
    .bind(&ur.name)
    .bind(&ur.description)
    .bind(ur.builtin)
    .fetch_one(pool)
    .await?;
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM platform.user_role_permission WHERE role_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if !ur.permissions.is_empty() {
        sqlx::query(
            "INSERT INTO platform.user_role_permission (role_id, permission) \
             SELECT $1, p FROM unnest($2::text[]) AS p",
        )
        .bind(id)
        .bind(&ur.permissions)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

async fn apply_user_role_assignment(
    pool: &PgPool,
    a: &UserRoleAssignmentDecl,
) -> Result<(), sqlx::Error> {
    let user_id: Option<Uuid> = if let Some(sub) = a.oidc_sub.as_deref() {
        sqlx::query_scalar("SELECT id FROM platform.\"user\" WHERE oidc_sub = $1")
            .bind(sub)
            .fetch_optional(pool)
            .await?
    } else if let Some(email) = a.email.as_deref() {
        sqlx::query_scalar("SELECT id FROM platform.\"user\" WHERE email = $1")
            .bind(email)
            .fetch_optional(pool)
            .await?
    } else {
        return Err(sqlx::Error::Protocol(
            "user_role_assignment needs either oidc_sub or email".to_owned(),
        ));
    };
    let Some(user_id) = user_id else {
        return Err(sqlx::Error::Protocol(
            "no platform.user row — has the user logged in once?".to_owned(),
        ));
    };
    let role_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM platform.user_role WHERE name = $1")
            .bind(&a.user_role)
            .fetch_optional(pool)
            .await?;
    let Some(role_id) = role_id else {
        return Err(sqlx::Error::Protocol(format!(
            "user_role {:?} not declared",
            a.user_role
        )));
    };
    sqlx::query(
        "INSERT INTO platform.user_role_assignment (user_id, role_id) \
         VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(role_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn apply_oidc_mapping(pool: &PgPool, m: &OidcMappingDecl) -> Result<(), sqlx::Error> {
    let group_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM platform.\"group\" WHERE name = $1 ORDER BY created_at LIMIT 1",
    )
    .bind(&m.group)
    .fetch_optional(pool)
    .await?;
    let Some(group_id) = group_id else {
        return Err(sqlx::Error::Protocol(format!(
            "group {:?} not declared",
            m.group
        )));
    };
    let role_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM platform.group_role WHERE group_id = $1 AND name = $2")
            .bind(group_id)
            .bind(&m.role)
            .fetch_optional(pool)
            .await?;
    let Some(role_id) = role_id else {
        return Err(sqlx::Error::Protocol(format!(
            "role {:?} not declared in group {:?}",
            m.role, m.group
        )));
    };
    sqlx::query(
        "INSERT INTO platform.oidc_group_mapping (oidc_group_name, group_id, role_id) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (oidc_group_name, group_id) DO UPDATE SET role_id = EXCLUDED.role_id",
    )
    .bind(&m.oidc_group)
    .bind(group_id)
    .bind(role_id)
    .execute(pool)
    .await?;
    Ok(())
}
