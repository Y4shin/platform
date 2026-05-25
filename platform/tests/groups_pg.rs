//! Integration test for the `Groups` SDK accessor (M13) against an ephemeral
//! Postgres. Seeds a group with members and exercises `by_name`/`by_id`/`members`.
//! Skips cleanly without Docker.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use junius_sdk::{GroupId, Groups, PluginError, UserId};
use sqlx::PgPool;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use uuid::Uuid;

const HOST_MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_users.up.sql"),
    include_str!("../migrations/0003_groups_roles_memberships.up.sql"),
];

async fn apply_migrations(pool: &PgPool) {
    for sql in HOST_MIGRATIONS {
        sqlx::raw_sql(sql).execute(pool).await.unwrap();
    }
}

/// Insert a user and return its id.
async fn seed_user(pool: &PgPool, sub: &str, email: &str, name: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO platform.user (oidc_sub, email, display_name) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(sub)
    .bind(email)
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Seed a "Committee" group with a "member" role and two members (Alice, Bob).
/// Returns `(group_id, alice_id)` — Alice serves as an authorized caller.
async fn seed_group(pool: &PgPool) -> (Uuid, Uuid) {
    let group_id: Uuid = sqlx::query_scalar(
        "INSERT INTO platform.group (name, description) \
         VALUES ('Committee', 'The organising committee') RETURNING id",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let role_id: Uuid = sqlx::query_scalar(
        "INSERT INTO platform.group_role (group_id, name) VALUES ($1, 'member') RETURNING id",
    )
    .bind(group_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let mut alice_id = Uuid::nil();
    for (sub, email, name) in [
        ("sub-alice", "alice@local", "Alice"),
        ("sub-bob", "bob@local", "Bob"),
    ] {
        let user_id = seed_user(pool, sub, email, name).await;
        if name == "Alice" {
            alice_id = user_id;
        }
        sqlx::query(
            "INSERT INTO platform.group_membership (user_id, group_id, role_id) \
             VALUES ($1, $2, $3)",
        )
        .bind(user_id)
        .bind(group_id)
        .bind(role_id)
        .execute(pool)
        .await
        .unwrap();
    }
    (group_id, alice_id)
}

#[tokio::test]
async fn groups_accessor_resolves_and_enumerates() {
    let node = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping groups_pg: Docker unavailable ({e})");
            return;
        }
    };
    let port = node.get_host_port_ipv4(5432).await.unwrap();
    let pool = PgPool::connect(&format!(
        "postgres://postgres:postgres@127.0.0.1:{port}/postgres"
    ))
    .await
    .unwrap();
    apply_migrations(&pool).await;
    let (group_id, alice_id) = seed_group(&pool).await;
    // Carol exists but is not a member of the Committee.
    let carol_id = seed_user(&pool, "sub-carol", "carol@local", "Carol").await;

    let groups = Groups::new(pool.clone());

    // by_name resolves to the seeded group.
    let by_name = groups.by_name("Committee").await.unwrap().expect("found");
    assert_eq!(by_name.id.0, group_id);
    assert_eq!(by_name.name, "Committee");
    assert_eq!(
        by_name.description.as_deref(),
        Some("The organising committee")
    );

    // by_id round-trips.
    let by_id = groups
        .by_id(GroupId(group_id))
        .await
        .unwrap()
        .expect("found");
    assert_eq!(by_id.name, "Committee");

    // is_member reflects membership.
    assert!(
        groups
            .is_member(GroupId(group_id), UserId(alice_id))
            .await
            .unwrap()
    );
    assert!(
        !groups
            .is_member(GroupId(group_id), UserId(carol_id))
            .await
            .unwrap()
    );

    // members (authorized): a member caller (Alice) sees both users, ordered by
    // display_name, with their role.
    let members = groups
        .members(GroupId(group_id), UserId(alice_id))
        .await
        .unwrap();
    assert_eq!(members.len(), 2);
    assert_eq!(members[0].user.display_name, "Alice");
    assert_eq!(members[1].user.display_name, "Bob");
    assert_eq!(members[0].role.name, "member");

    // A non-member caller (Carol) is denied the roster — no disclosure.
    assert!(matches!(
        groups.members(GroupId(group_id), UserId(carol_id)).await,
        Err(PluginError::PermissionDenied(_))
    ));

    // Unknown name resolves to None; an unknown group denies any caller (they are
    // not a member of a group that does not exist).
    assert!(groups.by_name("Nope").await.unwrap().is_none());
    assert!(matches!(
        groups.members(GroupId(Uuid::nil()), UserId(alice_id)).await,
        Err(PluginError::PermissionDenied(_))
    ));
}
