//! Integration tests for the migration runner against an ephemeral Postgres 17.
//!
//! These exercise the security-critical surface — least-privilege role grants and
//! real connection-level isolation — plus apply/idempotency/checksum behavior.
//! Skipped automatically (test passes with a note) when no Docker daemon is
//! reachable, so a Docker-less dev box still gets a green `cargo test`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use junius::migrate::{self, MigrateError};
use junius_manifest::{PluginManifest, ResolvedConfig, derive_role_password};
use sqlx::{Row, postgres::PgPoolOptions};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

const HOST_MIGRATIONS: &[(&str, &str)] = &[
    (
        "0001_users.up.sql",
        include_str!("../../../platform/migrations/0001_users.up.sql"),
    ),
    (
        "0002_sessions.up.sql",
        include_str!("../../../platform/migrations/0002_sessions.up.sql"),
    ),
    (
        "0003_groups_roles_memberships.up.sql",
        include_str!("../../../platform/migrations/0003_groups_roles_memberships.up.sql"),
    ),
    (
        "0004_resource_principal_share.up.sql",
        include_str!("../../../platform/migrations/0004_resource_principal_share.up.sql"),
    ),
    (
        "0005_user_can_access.up.sql",
        include_str!("../../../platform/migrations/0005_user_can_access.up.sql"),
    ),
    (
        "0006_meta_migrations.up.sql",
        include_str!("../../../platform/migrations/0006_meta_migrations.up.sql"),
    ),
    (
        "0007_audit_event.up.sql",
        include_str!("../../../platform/migrations/0007_audit_event.up.sql"),
    ),
];

const DEMO_MANIFEST: &str = "[plugin]\nname = \"demo\"\ndisplay_name = \"Demo\"\nmanifest_schema = 1\n\
     [exposes.tables.thing]\nschema = \"demo\"\n";

const DEMO_MIGRATION: &str =
    "CREATE SCHEMA demo;\nCREATE TABLE demo.thing (id UUID PRIMARY KEY);\n";

const ROLE_SECRET: &str = "test-role-secret";

fn write_repo(root: &Path) {
    let host = root.join("platform/migrations");
    fs::create_dir_all(&host).unwrap();
    for (name, sql) in HOST_MIGRATIONS {
        fs::write(host.join(name), sql).unwrap();
    }
    let demo = root.join("plugins/demo/migrations");
    fs::create_dir_all(&demo).unwrap();
    fs::write(demo.join("0001_demo.up.sql"), DEMO_MIGRATION).unwrap();
}

fn config(database_url: String) -> ResolvedConfig {
    ResolvedConfig {
        database_url,
        oidc_issuer: "http://localhost/".into(),
        oidc_client_id: "unused".into(),
        oidc_client_secret: "unused".into(),
        session_encryption_key: "unused".into(),
        role_password_secret: ROLE_SECRET.into(),
        bind_addr: None,
    }
}

fn manifests() -> BTreeMap<String, PluginManifest> {
    let mut m = BTreeMap::new();
    m.insert(
        "demo".to_string(),
        PluginManifest::parse(DEMO_MANIFEST).unwrap(),
    );
    m
}

async fn scalar_bool(url: &str, query: &str) -> bool {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(url)
        .await
        .unwrap();
    let row = sqlx::query(query).fetch_one(&pool).await.unwrap();
    row.try_get::<bool, _>(0).unwrap()
}

#[tokio::test]
async fn migrations_apply_grants_and_enforce_role_isolation() {
    // Skip cleanly when Docker isn't available.
    let node = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping migrate_pg: Docker unavailable ({e})");
            return;
        }
    };
    let port = node.get_host_port_ipv4(5432).await.unwrap();
    // testcontainers' postgres defaults: user/password/db all "postgres" (superuser),
    // which has the broad rights the platform_migrator role needs.
    let admin_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    let tmp = common::tempdir();
    let root = tmp.path();
    write_repo(root);
    let cfg = config(admin_url.clone());
    let mfs = manifests();
    let enabled = vec!["demo".to_string()];

    // 1. First apply: host + demo migrations, then grants.
    let report = migrate::up(root, &cfg, &mfs, &enabled).await.expect("up");
    assert!(!report.applied.is_empty(), "first run applies migrations");
    assert!(report.roles_granted.contains(&"role_demo".to_string()));

    // 2. Schemas + the access function exist.
    for schema in ["meta", "platform", "demo"] {
        assert!(
            scalar_bool(
                &admin_url,
                &format!(
                    "SELECT EXISTS(SELECT 1 FROM information_schema.schemata WHERE schema_name = '{schema}')"
                ),
            )
            .await,
            "schema {schema} should exist"
        );
    }
    assert!(
        scalar_bool(
            &admin_url,
            "SELECT EXISTS(SELECT 1 FROM pg_proc WHERE proname = 'user_can_access')",
        )
        .await,
        "user_can_access() should exist"
    );

    // 3. Least-privilege grants for role_demo.
    assert!(
        scalar_bool(
            &admin_url,
            "SELECT has_table_privilege('role_demo','platform.user','SELECT')"
        )
        .await,
        "role_demo may read platform.user"
    );
    assert!(
        !scalar_bool(
            &admin_url,
            "SELECT has_table_privilege('role_demo','platform.session','SELECT')"
        )
        .await,
        "role_demo may NOT read platform.session"
    );
    assert!(
        scalar_bool(
            &admin_url,
            "SELECT has_table_privilege('role_demo','demo.thing','INSERT')"
        )
        .await,
        "role_demo has full DML on its own schema"
    );

    // 4. Real connection-level isolation: connect AS role_demo.
    let pw = derive_role_password(ROLE_SECRET, "role_demo");
    let demo_url = format!("postgres://role_demo:{pw}@127.0.0.1:{port}/postgres");
    let demo_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&demo_url)
        .await
        .expect("role_demo can authenticate with the derived password");
    sqlx::query("SELECT id FROM platform.user LIMIT 1")
        .fetch_optional(&demo_pool)
        .await
        .expect("role_demo may read platform.user");
    let denied = sqlx::query("SELECT id FROM platform.session LIMIT 1")
        .fetch_optional(&demo_pool)
        .await;
    assert!(denied.is_err(), "role_demo must be denied platform.session");

    // 5. Wrong role-password secret fails authentication.
    let wrong_pw = derive_role_password("a-different-secret", "role_demo");
    let wrong_url = format!("postgres://role_demo:{wrong_pw}@127.0.0.1:{port}/postgres");
    assert!(
        PgPoolOptions::new()
            .max_connections(1)
            .connect(&wrong_url)
            .await
            .is_err(),
        "a wrong role_password_secret must fail auth"
    );

    // 6. Idempotency: a second up() applies nothing.
    let again = migrate::up(root, &cfg, &mfs, &enabled)
        .await
        .expect("second up");
    assert!(again.applied.is_empty(), "no pending migrations on re-run");

    // 7. Checksum mismatch: tamper an applied migration → hard error.
    fs::write(
        root.join("plugins/demo/migrations/0001_demo.up.sql"),
        format!("{DEMO_MIGRATION}-- tampered\n"),
    )
    .unwrap();
    match migrate::up(root, &cfg, &mfs, &enabled).await {
        Err(MigrateError::ChecksumMismatch { plugin, name }) => {
            assert_eq!(plugin, "demo");
            assert_eq!(name, "0001_demo");
        }
        other => panic!("expected ChecksumMismatch, got {other:?}"),
    }
}
