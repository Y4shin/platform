//! Integration test for the M18 admin fast-path inside
//! `platform.user_can_access`. A user with a user-role holding the literal
//! `'*'` permission must short-circuit ACL evaluation regardless of
//! ownership, group membership, or shares. Skips cleanly without Docker.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use junius_sdk::{Authz, Principal, UserId};
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

async fn seed_user(pool: &PgPool, sub: &str) -> UserId {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO platform.user (oidc_sub, email, display_name) \
         VALUES ($1, $1 || '@local', $1) RETURNING id",
    )
    .bind(sub)
    .fetch_one(pool)
    .await
    .unwrap();
    UserId(id)
}

async fn assign_admin(pool: &PgPool, user: UserId) {
    sqlx::query(
        "INSERT INTO platform.user_role_assignment (user_id, role_id) \
         SELECT $1, id FROM platform.user_role WHERE name = 'admin'",
    )
    .bind(user.0)
    .execute(pool)
    .await
    .unwrap();
}

async fn can_access(pool: &PgPool, rid: Uuid, user: UserId, perm: &str) -> bool {
    sqlx::query_scalar("SELECT platform.user_can_access('events:event', $1, $2, $3)")
        .bind(rid)
        .bind(user.0)
        .bind(perm)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn admin_wildcard_short_circuits_user_can_access() {
    let node = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping authz_admin_pg: Docker unavailable ({e})");
            return;
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

    let alice = seed_user(&pool, "alice").await;
    let bob = seed_user(&pool, "bob").await;
    let carol = seed_user(&pool, "carol").await;
    let rid = Uuid::new_v4();

    // Bob owns the resource; alice + carol have no ACL relationship to it.
    let bob_authz = Authz::new(pool.clone()).with_user(Some(bob));
    let mut tx = pool.begin().await.unwrap();
    bob_authz
        .record_owner(&mut tx, "events:event", rid, Principal::User(bob))
        .await
        .unwrap();
    tx.commit().await.unwrap();

    // Baseline: alice and carol are blocked, bob is allowed (as owner).
    assert!(can_access(&pool, rid, bob, "events:read").await);
    assert!(!can_access(&pool, rid, alice, "events:read").await);
    assert!(!can_access(&pool, rid, carol, "events:write").await);

    // Promote alice to admin. Carol stays non-admin.
    assign_admin(&pool, alice).await;

    // Admin lifts every permission for any resource — including permissions
    // that don't exist as concrete role grants anywhere in the DB.
    assert!(can_access(&pool, rid, alice, "events:read").await);
    assert!(can_access(&pool, rid, alice, "events:write").await);
    assert!(can_access(&pool, rid, alice, "events:share").await);
    assert!(can_access(&pool, rid, alice, "made-up:perm-that-no-one-declares").await);

    // Carol stays blocked — the admin user-role is per-user.
    assert!(!can_access(&pool, rid, carol, "events:read").await);
    assert!(!can_access(&pool, rid, carol, "events:write").await);

    // Owner path still works for bob (the fast-path doesn't break ownership).
    assert!(can_access(&pool, rid, bob, "events:write").await);
}

#[tokio::test]
async fn builtin_admin_role_is_seeded_once_per_migration() {
    let node = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping authz_admin_pg: Docker unavailable ({e})");
            return;
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

    // After one fresh migration run, the seed creates exactly one builtin
    // admin row with exactly the wildcard permission.
    let row_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform.user_role WHERE name = 'admin' AND is_builtin = true",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row_count, 1);
    let perm_rows: Vec<String> = sqlx::query_scalar(
        "SELECT urp.permission FROM platform.user_role_permission urp \
         JOIN platform.user_role ur ON ur.id = urp.role_id \
         WHERE ur.name = 'admin' ORDER BY urp.permission",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(perm_rows, vec!["*".to_string()]);

    // Re-running just the idempotent seed block (the same INSERT … ON CONFLICT
    // statements the migration ends with) must NOT duplicate rows. We exercise
    // the seed body directly rather than the whole migration, which carries a
    // non-idempotent CREATE TABLE.
    let reseed = r"
        INSERT INTO platform.user_role (name, description, is_builtin)
        VALUES ('admin', 'Full access to every permission in every group.', true)
        ON CONFLICT (name) DO UPDATE SET
            description = EXCLUDED.description, is_builtin = true;
        INSERT INTO platform.user_role_permission (role_id, permission)
        SELECT id, '*' FROM platform.user_role WHERE name = 'admin'
        ON CONFLICT DO NOTHING;
    ";
    sqlx::raw_sql(reseed).execute(&pool).await.unwrap();

    let row_count2: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform.user_role WHERE name = 'admin' AND is_builtin = true",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let perm_count2: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform.user_role_permission urp \
         JOIN platform.user_role ur ON ur.id = urp.role_id \
         WHERE ur.name = 'admin' AND urp.permission = '*'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row_count2, 1);
    assert_eq!(perm_count2, 1);
}
