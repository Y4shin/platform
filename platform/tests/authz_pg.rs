//! Integration test for the M08 `Authz` API + the SECURITY DEFINER access
//! functions, against an ephemeral Postgres. Skips cleanly without Docker.

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

async fn can_access(pool: &PgPool, rid: Uuid, user: UserId, perm: &str) -> bool {
    sqlx::query_scalar("SELECT platform.user_can_access('test:thing', $1, $2, $3)")
        .bind(rid)
        .bind(user.0)
        .bind(perm)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn audit_count(pool: &PgPool, kind: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM platform.audit_event WHERE event_kind = $1")
        .bind(kind)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn ownership_sharing_and_access() {
    let node = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping authz_pg: Docker unavailable ({e})");
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
    let rid = Uuid::new_v4();

    let alice_authz = Authz::new(pool.clone()).with_user(Some(alice));

    // record_owner inside a transaction (as a plugin would, alongside its INSERT).
    let mut tx = pool.begin().await.unwrap();
    alice_authz
        .record_owner(&mut tx, "test:thing", rid, Principal::User(alice))
        .await
        .unwrap();
    tx.commit().await.unwrap();

    // Owner has full access; a stranger has none.
    assert!(can_access(&pool, rid, alice, "test:write").await);
    assert!(!can_access(&pool, rid, bob, "test:read").await);

    // Alice shares read with bob.
    let share = alice_authz
        .share("test:thing", rid, Principal::User(bob), "test:read", None)
        .await
        .unwrap();
    assert!(can_access(&pool, rid, bob, "test:read").await);
    assert!(!can_access(&pool, rid, bob, "test:write").await); // only read was shared

    // A non-owner cannot share.
    let bob_authz = Authz::new(pool.clone()).with_user(Some(bob));
    let denied = bob_authz
        .share("test:thing", rid, Principal::Public, "test:read", None)
        .await;
    assert!(matches!(
        denied,
        Err(junius_sdk::PluginError::PermissionDenied(_))
    ));

    // Unshare revokes bob's access.
    alice_authz
        .unshare("test:thing", rid, share.id)
        .await
        .unwrap();
    assert!(!can_access(&pool, rid, bob, "test:read").await);

    // Share + unshare were audited.
    assert_eq!(audit_count(&pool, "test:thing.share").await, 1);
    assert_eq!(audit_count(&pool, "test:thing.unshare").await, 1);
}
