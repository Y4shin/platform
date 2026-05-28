//! Integration test for the M18 Stage D `lock_managed` enforcement on the
//! `PlatformAdminApi` seam. When the deployment's
//! `[provisioning] lock_managed = true` (default), mutations on a
//! `group_membership` row with `managed_by='config'` refuse with
//! [`PluginError::ManagedByConfig`]; the same call against a
//! `managed_by='manual'` row succeeds, and the same call against a
//! `managed_by='config'` row with `lock_managed = false` also succeeds.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use junius_sdk::{AuditEmitter, GroupId, PlatformAdminApi, PluginError, RoleId, UserId};
use sqlx::PgPool;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use uuid::Uuid;

const HOST_MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_users.up.sql"),
    include_str!("../migrations/0002_sessions.up.sql"),
    include_str!("../migrations/0003_groups_roles_memberships.up.sql"),
    include_str!("../migrations/0004_resource_principal_share.up.sql"),
    include_str!("../migrations/0005_user_can_access.up.sql"),
    include_str!("../migrations/0006_meta_migrations.up.sql"),
    include_str!("../migrations/0007_audit_event.up.sql"),
    include_str!("../migrations/0008_authz_functions.up.sql"),
    include_str!("../migrations/0011_user_locale.up.sql"),
    include_str!("../migrations/0012_forget_resource.up.sql"),
    include_str!("../migrations/0013_user_roles.up.sql"),
    include_str!("../migrations/0014_oidc_group_mapping.up.sql"),
];

const CAPS: &[&str] = &["platform.admin"];

async fn seed(pool: &PgPool) -> (UserId, GroupId, RoleId) {
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO platform.user (oidc_sub, email, display_name) \
         VALUES ('alice', 'alice@local', 'Alice') RETURNING id",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let group: Uuid = sqlx::query_scalar(
        "INSERT INTO platform.\"group\" (name, description) \
         VALUES ('Organisers', 'test') RETURNING id",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let role: Uuid = sqlx::query_scalar(
        "INSERT INTO platform.group_role (group_id, name) \
         VALUES ($1, 'organiser') RETURNING id",
    )
    .bind(group)
    .fetch_one(pool)
    .await
    .unwrap();
    (UserId(user), GroupId(group), RoleId(role))
}

async fn insert_membership(pool: &PgPool, user: UserId, group: GroupId, role: RoleId, by: &str) {
    sqlx::query(
        "INSERT INTO platform.group_membership \
            (user_id, group_id, role_id, managed_by, managed_source) \
         VALUES ($1, $2, $3, $4, NULL)",
    )
    .bind(user.0)
    .bind(group.0)
    .bind(role.0)
    .bind(by)
    .execute(pool)
    .await
    .unwrap();
}

fn admin_api(pool: &PgPool, lock_managed: bool) -> PlatformAdminApi {
    PlatformAdminApi::new(
        pool.clone(),
        AuditEmitter::new(pool.clone()),
        "admin",
        CAPS,
        Arc::new(Vec::new()),
        lock_managed,
    )
}

async fn pg_pool() -> Option<(testcontainers_modules::testcontainers::ContainerAsync<Postgres>, PgPool)> {
    let node = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping admin_lock_managed_pg: Docker unavailable ({e})");
            return None;
        }
    };
    let port = node.get_host_port_ipv4(5432).await.unwrap();
    let pool = PgPool::connect(&format!(
        "postgres://postgres:postgres@127.0.0.1:{port}/postgres"
    ))
    .await
    .unwrap();
    for sql in HOST_MIGRATIONS {
        sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    }
    Some((node, pool))
}

#[tokio::test]
async fn lock_managed_blocks_remove_on_config_row() {
    let Some((_node, pool)) = pg_pool().await else {
        return;
    };
    let (user, group, role) = seed(&pool).await;
    insert_membership(&pool, user, group, role, "config").await;

    let locked = admin_api(&pool, true);
    let err = locked.remove_group_member(group, user).await.unwrap_err();
    assert!(
        matches!(err, PluginError::ManagedByConfig(_)),
        "expected ManagedByConfig, got {err:?}",
    );

    // Row still present.
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform.group_membership WHERE group_id = $1 AND user_id = $2",
    )
    .bind(group.0)
    .bind(user.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn lock_managed_off_allows_remove_on_config_row() {
    let Some((_node, pool)) = pg_pool().await else {
        return;
    };
    let (user, group, role) = seed(&pool).await;
    insert_membership(&pool, user, group, role, "config").await;

    let unlocked = admin_api(&pool, false);
    unlocked.remove_group_member(group, user).await.unwrap();
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform.group_membership WHERE group_id = $1 AND user_id = $2",
    )
    .bind(group.0)
    .bind(user.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn lock_managed_allows_mutation_of_manual_rows() {
    let Some((_node, pool)) = pg_pool().await else {
        return;
    };
    let (user, group, role) = seed(&pool).await;
    insert_membership(&pool, user, group, role, "manual").await;

    let locked = admin_api(&pool, true);
    // 'manual' rows are unaffected by the lock — admin keeps full control.
    locked.remove_group_member(group, user).await.unwrap();
}

#[tokio::test]
async fn lock_managed_blocks_add_overwriting_config_row() {
    let Some((_node, pool)) = pg_pool().await else {
        return;
    };
    let (user, group, role) = seed(&pool).await;
    insert_membership(&pool, user, group, role, "config").await;

    let locked = admin_api(&pool, true);
    let err = locked
        .add_group_member(group, user, role)
        .await
        .unwrap_err();
    assert!(
        matches!(err, PluginError::ManagedByConfig(_)),
        "expected ManagedByConfig, got {err:?}",
    );
    // Row is still managed_by='config'.
    let managed: String = sqlx::query_scalar(
        "SELECT managed_by FROM platform.group_membership WHERE group_id = $1 AND user_id = $2",
    )
    .bind(group.0)
    .bind(user.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(managed, "config");
}
