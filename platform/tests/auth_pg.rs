//! Integration test for the host auth surface against an ephemeral Postgres.
//! Seeds a session directly (no live identity provider) and exercises the middleware,
//! `GET /api/me`, and `POST /api/auth/logout`. Skips cleanly without Docker.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use platform::auth::AuthState;
use platform::db::{DbBootstrap, PluginPools};
use platform::server;
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

async fn apply_migrations(pool: &PgPool) {
    for sql in HOST_MIGRATIONS {
        sqlx::raw_sql(sql).execute(pool).await.unwrap();
    }
}

/// Seed one user with a single group/role/permission and an unexpired session;
/// returns the session id.
async fn seed_session(pool: &PgPool) -> Uuid {
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO platform.user (oidc_sub, email, display_name) \
         VALUES ('sub-alice', 'alice@local', 'Alice') RETURNING id",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let group_id: Uuid =
        sqlx::query_scalar("INSERT INTO platform.group (name) VALUES ('Committee') RETURNING id")
            .fetch_one(pool)
            .await
            .unwrap();
    let role_id: Uuid = sqlx::query_scalar(
        "INSERT INTO platform.group_role (group_id, name) VALUES ($1, 'chair') RETURNING id",
    )
    .bind(group_id)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO platform.role_permission (role_id, permission) VALUES ($1, 'speakers:read')",
    )
    .bind(role_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO platform.group_membership (user_id, group_id, role_id) VALUES ($1, $2, $3)",
    )
    .bind(user_id)
    .bind(group_id)
    .bind(role_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query_scalar(
        "INSERT INTO platform.session (user_id, expires_at) \
         VALUES ($1, now() + interval '1 hour') RETURNING id",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn api_me_round_trip_and_logout() {
    let node = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping auth_pg: Docker unavailable ({e})");
            return;
        }
    };
    let port = node.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    let db = DbBootstrap::connect(&url).await.unwrap();
    let pool = db.platform_pool().clone();
    apply_migrations(&pool).await;
    let session_id = seed_session(&pool).await;

    let auth_state = AuthState::for_test(pool.clone(), "test-session-key");
    let localizer = junius_sdk::LocalizerBuilder::new(junius_sdk::Locale::En).build();
    let app = server::build_app_with_services(
        &[],
        &PluginPools::empty(),
        &pool,
        &std::collections::BTreeMap::new(),
        auth_state,
        &platform::infra::HostInfra::default(),
        &localizer,
    );

    // /api/me with a valid session cookie → 200 + camelCase nested JSON.
    let resp = app
        .clone()
        .oneshot(
            Request::get("/api/me")
                .header(header::COOKIE, format!("session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    let me: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(me["email"], "alice@local");
    assert_eq!(me["displayName"], "Alice");
    assert_eq!(me["memberships"][0]["groupName"], "Committee");
    assert_eq!(me["memberships"][0]["role"]["name"], "chair");
    let perms = me["memberships"][0]["permissions"].as_array().unwrap();
    assert!(perms.iter().any(|p| p == "speakers:read"));

    // No cookie → 401.
    let resp = app
        .clone()
        .oneshot(Request::get("/api/me").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Logout deletes the session row.
    let resp = app
        .clone()
        .oneshot(
            Request::post("/api/auth/logout")
                .header(header::COOKIE, format!("session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM platform.session WHERE id = $1")
        .bind(session_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, 0, "logout should delete the session row");

    // A request bearing the now-deleted session is unauthenticated.
    let resp = app
        .oneshot(
            Request::get("/api/me")
                .header(header::COOKIE, format!("session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
