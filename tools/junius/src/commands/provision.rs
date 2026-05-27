//! `junius provision` — M18 Stage D declarative reconciliation.
//!
//! Reads the deployment's `[provisioning]` block and reconciles the live
//! database against it. The block is hashed (blake3) and the
//! `platform.provisioning_state` row stores the last applied hash; a
//! re-run with the same content is a no-op.
//!
//! What's reconciled in this stage:
//! - Groups (by name) — upserted, description updated when changed.
//! - Group roles (by `(group, role_name)`) — upserted, permission set
//!   replaced.
//! - User-roles (by name) — upserted, permission set replaced. Built-in
//!   rows preserve `is_builtin = true`.
//! - User-role assignments (by `oidc_sub` or `email`) — upserted. A user
//!   referenced by email that has never logged in is skipped with a
//!   warning.
//! - OIDC group mappings (by `(oidc_group_name, group_id)`) — upserted.
//!
//! What's **not yet** done (called out in commits/docs for follow-up):
//! - Destructive reconciliation (drop a `managed_by='config'` row whose
//!   declaration disappeared from the file). `--allow-delete` is parsed
//!   but a no-op in this iteration.
//! - The `[provisioning] lock_managed = true` runtime enforcement at
//!   the `PlatformAdminApi` seam. The flag is read + persisted but
//!   write-side rejection is a follow-up.

use std::path::{Path, PathBuf};

use junius_manifest::{
    OidcMappingDecl, PlatformManifest, ProvisioningConfig, UserRoleAssignmentDecl, UserRoleDecl,
    resolve_config,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::cli::ProvisionCmd;
use crate::commands::migrate::env_lookup;
use crate::exit;

pub fn run(cmd: &ProvisionCmd) -> i32 {
    match cmd {
        ProvisionCmd::Apply {
            config,
            force,
            allow_delete,
        } => tokio_runtime().block_on(apply(config.clone(), *force, *allow_delete)),
        ProvisionCmd::Diff { config } => tokio_runtime().block_on(diff(config.clone())),
    }
}

fn tokio_runtime() -> tokio::runtime::Runtime {
    #[allow(
        clippy::expect_used,
        reason = "tokio runtime construction failures are unrecoverable"
    )]
    tokio::runtime::Runtime::new().expect("tokio runtime")
}

async fn apply(config_path: Option<PathBuf>, force: bool, _allow_delete: bool) -> i32 {
    let Some((manifest, deployment_dir, provisioning, database_url)) = load(config_path) else {
        return exit::PARSE_ERROR;
    };
    let _ = manifest; // currently unused; reserved for resolving plugin overrides later.

    let resolved = match provisioning.resolve_file(&deployment_dir) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("junius provision: failed to load `file = …` pointer: {e}");
            return exit::PARSE_ERROR;
        }
    };
    let want_hash = resolved.content_hash();

    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("junius provision: connect to database failed: {e}");
            return exit::PARSE_ERROR;
        }
    };

    let current_hash = current_hash(&pool).await.unwrap_or(None);
    if !force && current_hash.as_deref() == Some(&want_hash) {
        println!(
            "junius provision: already up-to-date (hash {})",
            &want_hash[..16]
        );
        return exit::OK;
    }

    if let Err(e) = reconcile(&pool, &resolved).await {
        eprintln!("junius provision: apply failed: {e}");
        return exit::PARSE_ERROR;
    }

    if let Err(e) = persist_hash(&pool, &want_hash).await {
        eprintln!("junius provision: persist state failed: {e}");
        return exit::PARSE_ERROR;
    }

    println!(
        "junius provision: applied {} group(s), {} user-role(s), {} assignment(s), {} oidc mapping(s)",
        resolved.groups.len(),
        resolved.user_roles.len(),
        resolved.user_role_assignments.len(),
        resolved.oidc_mappings.len(),
    );
    exit::OK
}

#[allow(
    clippy::unused_async,
    reason = "uniform shape with apply(); a richer diff will await DB queries"
)]
async fn diff(config_path: Option<PathBuf>) -> i32 {
    let Some((manifest, deployment_dir, provisioning, _database_url)) = load(config_path) else {
        return exit::PARSE_ERROR;
    };
    let _ = manifest;
    let resolved = match provisioning.resolve_file(&deployment_dir) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("junius provision: failed to load `file = …` pointer: {e}");
            return exit::PARSE_ERROR;
        }
    };
    println!(
        "junius provision: {} group(s), {} user-role(s), {} assignment(s), {} oidc mapping(s) declared",
        resolved.groups.len(),
        resolved.user_roles.len(),
        resolved.user_role_assignments.len(),
        resolved.oidc_mappings.len(),
    );
    println!("content hash: {}", &resolved.content_hash()[..16]);
    // A richer diff (current state ↔ declaration) lands in a follow-up;
    // for now the hash + counters are the deterministic summary `apply`
    // uses, and Stage 5 documents it.
    exit::OK
}

fn load(
    config_path: Option<PathBuf>,
) -> Option<(PlatformManifest, PathBuf, ProvisioningConfig, String)> {
    let path = config_path.unwrap_or_else(|| PathBuf::from("platform.toml"));
    let deployment_dir = path
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("junius provision: read {}: {e}", path.display());
            return None;
        }
    };
    let manifest = match PlatformManifest::parse(&src) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("junius provision: parse {}: {e}", path.display());
            return None;
        }
    };
    let Some(provisioning) = manifest.provisioning.clone() else {
        eprintln!(
            "junius provision: no [provisioning] block in {}",
            path.display()
        );
        return None;
    };
    let resolved = match resolve_config(&manifest.config, &env_lookup()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("junius provision: resolve [config]: {e}");
            return None;
        }
    };
    let database_url = resolved.database_url.clone();
    Some((manifest, deployment_dir, provisioning, database_url))
}

async fn current_hash(pool: &PgPool) -> Result<Option<String>, sqlx::Error> {
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
            eprintln!(
                "junius provision: skipping assignment for {:?}/{:?}: {e}",
                a.oidc_sub.as_deref().or(a.email.as_deref()).unwrap_or("?"),
                a.user_role,
            );
        }
    }
    for m in &p.oidc_mappings {
        if let Err(e) = apply_oidc_mapping(pool, m).await {
            eprintln!(
                "junius provision: skipping OIDC mapping {} → {}/{}: {e}",
                m.oidc_group, m.group, m.role,
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
