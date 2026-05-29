//! Integration test for the host-owned `user.v1.UserService` Connect-RPC
//! (M18) against an ephemeral Postgres. Focuses on the two auth modes — the
//! self-refresh method needs a session; the admin sweep needs the admin API
//! token — without standing up a live `IdP` (the reconcile happy-path is covered
//! by the OIDC reconcile tests + the e2e/manual verification). Skips cleanly
//! without Docker.

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

const ADMIN_TOKEN: &str = "secret-admin-token";

const HOST_MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_users.up.sql"),
    include_str!("../migrations/0002_sessions.up.sql"),
    include_str!("../migrations/0003_groups_roles_memberships.up.sql"),
    include_str!("../migrations/0004_resource_principal_share.up.sql"),
    include_str!("../migrations/0005_user_can_access.up.sql"),
    include_str!("../migrations/0006_meta_migrations.up.sql"),
    include_str!("../migrations/0007_audit_event.up.sql"),
    include_str!("../migrations/0008_authz_functions.up.sql"),
    include_str!("../migrations/0009_job_run.up.sql"),
    include_str!("../migrations/0010_object.up.sql"),
    include_str!("../migrations/0011_user_locale.up.sql"),
    include_str!("../migrations/0012_forget_resource.up.sql"),
    include_str!("../migrations/0013_user_roles.up.sql"),
    include_str!("../migrations/0014_oidc_group_mapping.up.sql"),
];

async fn apply_migrations(pool: &PgPool) {
    for sql in HOST_MIGRATIONS {
        sqlx::raw_sql(sql).execute(pool).await.unwrap();
    }
}

/// Seed one user + an unexpired session (no stored OIDC token); returns the
/// session id.
async fn seed_session(pool: &PgPool) -> Uuid {
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO platform.user (oidc_sub, email, display_name) \
         VALUES ('sub-alice', 'alice@local', 'Alice') RETURNING id",
    )
    .fetch_one(pool)
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

fn rpc(path: &str) -> axum::http::request::Builder {
    Request::post(format!("/rpc/user.v1.{path}")).header(header::CONTENT_TYPE, "application/json")
}

#[tokio::test]
async fn user_service_auth_modes() {
    let node = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping user_service_pg: Docker unavailable ({e})");
            return;
        }
    };
    let port = node.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    let db = DbBootstrap::connect(&url).await.unwrap();
    let pool = db.platform_pool().clone();
    apply_migrations(&pool).await;
    let session_id = seed_session(&pool).await;

    let auth_state =
        AuthState::for_test_with_admin_token(pool.clone(), "test-session-key", ADMIN_TOKEN);
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

    // RefreshOidcGroups without a session → Unauthenticated (401).
    let resp = app
        .clone()
        .oneshot(
            rpc("UserService/RefreshOidcGroups")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "self-refresh without a session is unauthenticated"
    );

    // ResyncOidcGroups without the admin bearer → Unauthenticated (401), even
    // with a valid session cookie (the sweep needs the token, not a session).
    let resp = app
        .clone()
        .oneshot(
            rpc("UserService/ResyncOidcGroups")
                .header(header::COOKIE, format!("session={session_id}"))
                .body(Body::from(r#"{"all":true}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "admin sweep without the admin token is unauthenticated"
    );

    // ResyncOidcGroups with the admin bearer + `all` → 200. No session carries
    // a stored token, so the target set is empty and the sweep is a clean no-op
    // (it never touches the unconfigured userinfo endpoint). This proves the
    // admin-token middleware → HostCtx::is_admin_token() → handler path works.
    let resp = app
        .clone()
        .oneshot(
            rpc("UserService/ResyncOidcGroups")
                .header(header::AUTHORIZATION, format!("Bearer {ADMIN_TOKEN}"))
                .body(Body::from(r#"{"all":true}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "admin sweep with the token succeeds (empty target set)"
    );
    let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let results = body
        .get("results")
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len);
    assert_eq!(results, 0, "no token-bearing sessions ⇒ empty sweep");

    // Wrong admin token → Unauthenticated.
    let resp = app
        .oneshot(
            rpc("UserService/ResyncOidcGroups")
                .header(header::AUTHORIZATION, "Bearer not-the-token")
                .body(Body::from(r#"{"all":true}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "a wrong admin token is unauthenticated"
    );
}
