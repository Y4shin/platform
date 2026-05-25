//! Cross-plugin integration tests for the greetings plugin against an ephemeral
//! Postgres. The first test proves `GreetingRepo` reads `hello.greeting_template`
//! via a cross-plugin JOIN; the second proves the least-privilege grant — a
//! `role_greetings` connection can `SELECT` `hello.greeting_template` (a declared
//! dep table) but not `hello.greeting` (not exposed to it). Both skip cleanly
//! (pass with a note) when no Docker daemon is reachable.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;

use greetings_plugin::repo::{GreetingRepo, NewGreeting};
use junius_manifest::{PluginManifest, compute_grants, derive_role_password, emit_grant_sql};
use junius_sdk::{AuditEmitter, PluginDb, User, UserId};
use sqlx::{PgPool, Row};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

const HOST_MIGRATIONS: &[&str] = &[
    include_str!("../../../platform/migrations/0001_users.up.sql"),
    include_str!("../../../platform/migrations/0002_sessions.up.sql"),
    include_str!("../../../platform/migrations/0003_groups_roles_memberships.up.sql"),
    include_str!("../../../platform/migrations/0004_resource_principal_share.up.sql"),
    include_str!("../../../platform/migrations/0005_user_can_access.up.sql"),
    include_str!("../../../platform/migrations/0006_meta_migrations.up.sql"),
    include_str!("../../../platform/migrations/0007_audit_event.up.sql"),
];
// hello must migrate before greetings (the FK target lives in hello:0003).
const HELLO_MIGRATIONS: &[&str] = &[
    include_str!("../../hello/migrations/0001_greeting.up.sql"),
    include_str!("../../hello/migrations/0002_note.up.sql"),
    include_str!("../../hello/migrations/0003_greeting_template.up.sql"),
];
const GREETINGS_MIGRATION: &str = include_str!("../migrations/0001_greeting.up.sql");

const HELLO_TOML: &str = include_str!("../../hello/plugin.toml");
const GREETINGS_TOML: &str = include_str!("../plugin.toml");
const ROLE_SECRET: &str = "test-role-secret";

/// A full greetings witness (read + write).
type ReadWrite = junius_sdk::permissions!(
    greetings_plugin::permissions::GreetingsRead & greetings_plugin::permissions::GreetingsWrite
);

/// Apply host + hello + greetings migrations (as the container superuser).
async fn migrate_all(pool: &PgPool) {
    for sql in HOST_MIGRATIONS
        .iter()
        .chain(HELLO_MIGRATIONS)
        .chain([GREETINGS_MIGRATION].iter())
    {
        sqlx::raw_sql(sql).execute(pool).await.unwrap();
    }
}

#[tokio::test]
async fn greeting_repo_reads_hello_template_cross_plugin() {
    let node = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping greetings_cross_plugin_pg: Docker unavailable ({e})");
            return;
        }
    };
    let port = node.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPool::connect(&url).await.unwrap();
    migrate_all(&pool).await;

    let user_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO platform.user (oidc_sub, email, display_name) \
         VALUES ('sub-amy', 'amy@local', 'Amy') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let user = User {
        id: UserId(user_id),
        email: "amy@local".into(),
        display_name: "Amy".into(),
        memberships: vec![],
    };

    let db = PluginDb::new(pool.clone(), "greetings");
    let audit = AuditEmitter::new(pool.clone());
    let repo: GreetingRepo<ReadWrite> = GreetingRepo::new(&db, Some(user), audit);

    // create resolves template_name -> hello.greeting_template.id (cross-plugin).
    let created = repo
        .create(NewGreeting {
            template_name: "default".into(),
            recipient: "alice".into(),
            venue: Some("Main Hall".into()),
        })
        .await
        .unwrap();
    assert_eq!(created.template_name, "default");
    assert_eq!(created.message, "Hello, alice!"); // rendered from hello's template body
    assert_eq!(created.venue.as_deref(), Some("Main Hall"));

    // list reads back through the same JOIN.
    let all = repo.list().await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, created.id);
    assert_eq!(all[0].message, "Hello, alice!");

    // An unknown template name resolves to no row → NotFound.
    let missing = repo
        .create(NewGreeting {
            template_name: "nope".into(),
            recipient: "bob".into(),
            venue: None,
        })
        .await;
    assert!(matches!(missing, Err(junius_sdk::RepoError::NotFound)));
}

#[tokio::test]
async fn role_greetings_reads_template_but_not_hello_greeting() {
    let node = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping greetings_cross_plugin_pg: Docker unavailable ({e})");
            return;
        }
    };
    let port = node.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let admin = PgPool::connect(&url).await.unwrap();
    migrate_all(&admin).await;

    // Compute + apply greetings' least-privilege role grant.
    let mut all = BTreeMap::new();
    all.insert(
        "hello".to_string(),
        PluginManifest::parse(HELLO_TOML).unwrap(),
    );
    let greetings = PluginManifest::parse(GREETINGS_TOML).unwrap();
    all.insert("greetings".to_string(), greetings.clone());

    let grant = compute_grants(&greetings, &all);
    let password = derive_role_password(ROLE_SECRET, &grant.role);
    sqlx::raw_sql(&emit_grant_sql(&grant, &password))
        .execute(&admin)
        .await
        .unwrap();

    // Connect as role_greetings and probe cross-plugin visibility.
    let role_url = format!(
        "postgres://{}:{password}@127.0.0.1:{port}/postgres",
        grant.role
    );
    let role_pool = PgPool::connect(&role_url).await.unwrap();

    // Declared dep table: SELECT is granted.
    let template_count: i64 = sqlx::query("SELECT count(*) FROM hello.greeting_template")
        .fetch_one(&role_pool)
        .await
        .unwrap()
        .get(0);
    assert!(template_count >= 2, "seeded templates should be visible");

    // Non-exposed hello table: SELECT is denied (role has USAGE on the schema but
    // no privilege on this table).
    let denied = sqlx::query("SELECT count(*) FROM hello.greeting")
        .fetch_one(&role_pool)
        .await;
    assert!(
        denied.is_err(),
        "role_greetings must not read hello.greeting"
    );
}
