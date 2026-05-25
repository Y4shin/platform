//! Functional test for the M07 `#[derive(PluginCtx)]` extractor over a real
//! HTTP route: `GET /h/hello/greetings` (mounted here unprefixed) requires
//! `hello:read`. A reader gets 200, a permission-less user 403, an anonymous
//! request 401. Skips cleanly without Docker.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashSet;

use axum::Extension;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use hello_plugin::HelloPlugin;
use junius_sdk::{
    AuditEmitter, Auth, Authz, Email, Groups, Jobs, Plugin, PluginConfig, PluginDb,
    PluginResourceCtx, PluginStorage, SecretStore, Telemetry, Users,
};
use junius_sdk::{GroupId, Membership, Role, RoleId, User, UserId};
use sqlx::PgPool;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tower::ServiceExt;
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

fn user_with(perms: &[&str]) -> User {
    User {
        id: UserId(Uuid::nil()),
        email: "u@example.com".into(),
        display_name: "U".into(),
        memberships: vec![Membership {
            group_id: GroupId(Uuid::nil()),
            group_name: "G".into(),
            role: Role {
                id: RoleId(Uuid::nil()),
                name: "r".into(),
            },
            permissions: perms
                .iter()
                .map(|p| (*p).to_string())
                .collect::<HashSet<_>>(),
        }],
    }
}

#[tokio::test]
async fn greetings_route_enforces_hello_read() {
    let node = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping hello_http_pg: Docker unavailable ({e})");
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
    sqlx::raw_sql(HELLO_MIGRATION).execute(&pool).await.unwrap();

    // The per-plugin context the host normally attaches per request.
    let ctx = PluginResourceCtx::new(
        PluginConfig::empty(),
        Telemetry::new("hello"),
        PluginDb::new(pool.clone(), "hello"),
        Auth::new(pool.clone()),
        Users::new(pool.clone()),
        Groups::new(pool.clone()),
        AuditEmitter::new(pool.clone()),
        Authz::new(pool.clone()),
        Email::disabled("hello", &[]),
        Jobs::disabled("hello", &[]),
        PluginStorage::empty("hello", &[]),
        SecretStore::default(),
        &[],
    );

    let get = |with_user: Option<User>| {
        let mut app = HelloPlugin::new().routes().layer(Extension(ctx.clone()));
        if let Some(u) = with_user {
            app = app.layer(Extension(u));
        }
        app.oneshot(Request::get("/greetings").body(Body::empty()).unwrap())
    };

    // Reader → 200 with an (empty) JSON array.
    let resp = get(Some(user_with(&["hello:read"]))).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    assert_eq!(&bytes[..], b"[]");

    // Authenticated but missing hello:read → 403.
    let resp = get(Some(user_with(&["hello:write"]))).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // Anonymous → 401.
    let resp = get(None).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
