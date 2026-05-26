//! M08 end-to-end: the note repo + `Authz` over a *least-privilege* `role_hello`
//! pool, proving the SECURITY DEFINER access functions work for plugin roles.
//! Covers the multi-user access flow (owner sees, stranger doesn't, share grants
//! read, non-owner can't share). Skips cleanly without Docker.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::similar_names)]

use hello_plugin::repo::{NewNote, NoteRepo};
use junius_sdk::{AuditEmitter, Authz, PluginDb, Principal, User, UserId};
use sqlx::PgPool;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use uuid::Uuid;

type ReadWrite = junius_sdk::permissions!(
    hello_plugin::permissions::HelloRead & hello_plugin::permissions::HelloWrite
);
type ReadOnly = junius_sdk::permissions!(hello_plugin::permissions::HelloRead);

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
const HELLO_MIGRATIONS: &[&str] = &[
    include_str!("../../plugins/hello/migrations/0001_greeting.up.sql"),
    include_str!("../../plugins/hello/migrations/0002_note.up.sql"),
];

// Mirrors the least-privilege set the migration runner's emit_grant_sql produces
// for a plugin role: own-schema DML + USAGE on platform + SELECT on platform.user
// (and EXECUTE on the access functions comes from the default PUBLIC grant).
const ROLE_HELLO_GRANTS: &str = "\
    CREATE ROLE role_hello LOGIN PASSWORD 'testpw' NOINHERIT; \
    GRANT USAGE, CREATE ON SCHEMA hello TO role_hello; \
    GRANT ALL ON ALL TABLES IN SCHEMA hello TO role_hello; \
    GRANT USAGE ON SCHEMA platform TO role_hello; \
    GRANT SELECT ON platform.user TO role_hello;";

async fn seed_user(pool: &PgPool, sub: &str) -> User {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO platform.user (oidc_sub, email, display_name) \
         VALUES ($1, $1 || '@local', $1) RETURNING id",
    )
    .bind(sub)
    .fetch_one(pool)
    .await
    .unwrap();
    User {
        id: UserId(id),
        email: format!("{sub}@local"),
        display_name: sub.to_string(),
        locale: None,
        memberships: vec![],
    }
}

#[tokio::test]
async fn note_ownership_and_sharing_via_role_hello() {
    let node = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping note_access_pg: Docker unavailable ({e})");
            return;
        }
    };
    let port = node.get_host_port_ipv4(5432).await.unwrap();
    let admin = PgPool::connect(&format!(
        "postgres://postgres:postgres@127.0.0.1:{port}/postgres"
    ))
    .await
    .unwrap();
    for sql in HOST_MIGRATIONS.iter().chain(HELLO_MIGRATIONS) {
        sqlx::raw_sql(sql).execute(&admin).await.unwrap();
    }
    sqlx::raw_sql(ROLE_HELLO_GRANTS)
        .execute(&admin)
        .await
        .unwrap();

    let alice = seed_user(&admin, "alice").await;
    let bob = seed_user(&admin, "bob").await;

    // The plugin's least-privilege pool + the host's platform pool (for Authz).
    let role_pool = PgPool::connect(&format!(
        "postgres://role_hello:testpw@127.0.0.1:{port}/postgres"
    ))
    .await
    .unwrap();
    let db = PluginDb::new(role_pool, "hello");
    let audit = AuditEmitter::new(admin.clone());
    let authz = |u: &User| Authz::new(admin.clone()).with_user(Some(u.id));

    let alice_rw: NoteRepo<ReadWrite> = NoteRepo::new(&db, Some(alice.clone()), audit.clone());
    let alice_ro: NoteRepo<ReadOnly> = NoteRepo::new(&db, Some(alice.clone()), audit.clone());
    let bob_ro: NoteRepo<ReadOnly> = NoteRepo::new(&db, Some(bob.clone()), audit.clone());

    // Alice creates a note (records ownership through the definer fn, as role_hello).
    let created = alice_rw
        .create(
            NewNote {
                title: "private".into(),
                body: "shh".into(),
            },
            &authz(&alice),
        )
        .await
        .unwrap();
    assert!(created.viewer_can_edit && created.viewer_can_share);

    // Alice sees it; bob doesn't; bob's direct get is NotFound (no existence leak).
    assert_eq!(alice_ro.list().await.unwrap().len(), 1);
    assert!(bob_ro.list().await.unwrap().is_empty());
    assert!(matches!(
        bob_ro.get(created.id).await,
        Err(junius_sdk::RepoError::NotFound)
    ));

    // Alice shares read with bob → bob now sees it, but can't edit/share.
    authz(&alice)
        .share(
            "hello:note",
            created.id.0,
            Principal::User(bob.id),
            "hello:read",
            None,
        )
        .await
        .unwrap();
    let bob_notes = bob_ro.list().await.unwrap();
    assert_eq!(bob_notes.len(), 1);
    assert!(!bob_notes[0].viewer_can_edit && !bob_notes[0].viewer_can_share);

    // Bob (not the owner) cannot share.
    let denied = authz(&bob)
        .share(
            "hello:note",
            created.id.0,
            Principal::Public,
            "hello:read",
            None,
        )
        .await;
    assert!(matches!(
        denied,
        Err(junius_sdk::PluginError::PermissionDenied(_))
    ));

    // The create + share were audited.
    let kinds: Vec<String> =
        sqlx::query_scalar("SELECT event_kind FROM platform.audit_event ORDER BY event_kind")
            .fetch_all(&admin)
            .await
            .unwrap();
    assert!(kinds.contains(&"hello:note.share".to_string()));
}
