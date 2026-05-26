//! Integration test for the M07 repository pattern against an ephemeral
//! Postgres: exercises `HelloRepo` create/list/get and confirms a write records
//! an audit event. Skips cleanly without Docker. (Compile-time permission gating
//! — that `HelloRepo<HelloRead>` has no `create` — is covered by the hello
//! plugin's trybuild tests.)

#![allow(clippy::unwrap_used, clippy::expect_used)]

use hello_plugin::repo::{GreetingId, HelloRepo, NewGreeting};
use junius_sdk::{AuditEmitter, PluginDb, User, UserId};
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
];
const HELLO_MIGRATION: &str = include_str!("../../plugins/hello/migrations/0001_greeting.up.sql");

/// A full-permission witness: holds both hello:read and hello:write.
type ReadWrite = junius_sdk::permissions!(
    hello_plugin::permissions::HelloRead & hello_plugin::permissions::HelloWrite
);

#[tokio::test]
async fn repo_create_list_get_and_audit() {
    let node = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping hello_repo_pg: Docker unavailable ({e})");
            return;
        }
    };
    let port = node.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPool::connect(&url).await.unwrap();

    for sql in HOST_MIGRATIONS {
        sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    }
    sqlx::raw_sql(HELLO_MIGRATION).execute(&pool).await.unwrap();

    // A user to attribute the audit event to (actor_user_id references platform.user).
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO platform.user (oidc_sub, email, display_name) \
         VALUES ('sub-bob', 'bob@local', 'Bob') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let user = User {
        id: UserId(user_id),
        email: "bob@local".into(),
        display_name: "Bob".into(),
        locale: None,
        memberships: vec![],
    };

    // The repository connects through a PluginDb; the host normally builds this
    // against the role_hello pool — here the superuser pool is fine to exercise
    // the repo logic (role isolation is covered by tools/junius migrate tests).
    let db = PluginDb::new(pool.clone(), "hello");
    let audit = AuditEmitter::new(pool.clone());
    let repo: HelloRepo<ReadWrite> = HelloRepo::new(&db, Some(user), audit);

    // create
    let created = repo
        .create(NewGreeting {
            name: "alice".into(),
            body: "hi".into(),
        })
        .await
        .unwrap();
    assert_eq!(created.name, "alice");
    assert_eq!(created.body, "hi");

    // list contains it
    let all = repo.list().await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, created.id);

    // get by id
    let got = repo.get(created.id).await.unwrap();
    assert_eq!(got.id, created.id);

    // missing → NotFound
    let missing = repo.get(GreetingId(Uuid::nil())).await;
    assert!(matches!(missing, Err(junius_sdk::RepoError::NotFound)));

    // the write recorded an audit event
    let audit_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform.audit_event \
         WHERE event_kind = 'hello:greeting.create' AND actor_user_id = $1",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_rows, 1);
}
