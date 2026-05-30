//! Integration test for the `/me` session surface against an ephemeral Postgres.
//! Covers the backend authorization gates of the M19 `/me` profile page:
//! `GET /api/me` now exposes `oidcSub` + a `sessions` array, and
//! `DELETE /api/sessions/<id>` revokes a session with a self-lockout guard on
//! the caller's *current* session. Seeds sessions directly (no live `IdP`) and
//! skips cleanly without Docker, mirroring `auth_pg.rs`.

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
    include_str!("../migrations/0008_authz_functions.up.sql"),
    include_str!("../migrations/0009_job_run.up.sql"),
    include_str!("../migrations/0010_object.up.sql"),
    include_str!("../migrations/0011_user_locale.up.sql"),
    include_str!("../migrations/0012_forget_resource.up.sql"),
    include_str!("../migrations/0013_user_roles.up.sql"),
    include_str!("../migrations/0014_oidc_group_mapping.up.sql"),
    include_str!("../migrations/0015_provisioning_state.up.sql"),
    include_str!("../migrations/0016_session_user_agent_last_seen.up.sql"),
];

async fn apply_migrations(pool: &PgPool) {
    for sql in HOST_MIGRATIONS {
        sqlx::raw_sql(sql).execute(pool).await.unwrap();
    }
}

/// Insert a user with the given `oidc_sub`/email/`display_name`; returns its id.
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

/// Insert an unexpired session for `user_id` carrying `user_agent`; returns its id.
async fn seed_session(pool: &PgPool, user_id: Uuid, user_agent: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO platform.session (user_id, expires_at, user_agent) \
         VALUES ($1, now() + interval '1 hour', $2) RETURNING id",
    )
    .bind(user_id)
    .bind(user_agent)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn session_exists(pool: &PgPool, id: Uuid) -> bool {
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM platform.session WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap();
    n == 1
}

fn build_test_app(pool: &PgPool) -> axum::Router {
    let auth_state = AuthState::for_test(pool.clone(), "test-session-key");
    let localizer = junius_sdk::LocalizerBuilder::new(junius_sdk::Locale::En).build();
    server::build_app_with_services(
        &[],
        &PluginPools::empty(),
        pool,
        &std::collections::BTreeMap::new(),
        auth_state,
        &platform::infra::HostInfra::default(),
        &localizer,
    )
}

async fn get_json(app: &axum::Router, path: &str, cookie: Option<Uuid>) -> (StatusCode, Body) {
    let mut req = Request::get(path);
    if let Some(id) = cookie {
        req = req.header(header::COOKIE, format!("session={id}"));
    }
    let resp = app
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    (resp.status(), resp.into_body())
}

async fn delete_status(app: &axum::Router, path: &str, cookie: Option<Uuid>) -> StatusCode {
    let mut req = Request::delete(path);
    if let Some(id) = cookie {
        req = req.header(header::COOKIE, format!("session={id}"));
    }
    app.clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn me_sessions_and_revoke_authorization() {
    let node = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping sessions_pg: Docker unavailable ({e})");
            return;
        }
    };
    let port = node.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    let db = DbBootstrap::connect(&url).await.unwrap();
    let pool = db.platform_pool().clone();
    apply_migrations(&pool).await;

    // Alice has two live sessions: the current one (the request's cookie) and an
    // older one she should be able to revoke. Bob is a second user whose session
    // Alice must not be able to touch.
    let alice = seed_user(&pool, "sub-alice", "alice@local", "Alice").await;
    let bob = seed_user(&pool, "sub-bob", "bob@local", "Bob").await;
    let current = seed_session(&pool, alice, "Chrome/Linux").await;
    let other = seed_session(&pool, alice, "Firefox/macOS").await;
    let bob_session = seed_session(&pool, bob, "Safari/iOS").await;

    let app = build_test_app(&pool);

    // GET /api/me exposes oidcSub + a sessions array; the request's own session
    // is flagged so the FE can suppress its Revoke control.
    let (status, body) = get_json(&app, "/api/me", Some(current)).await;
    assert_eq!(status, StatusCode::OK);
    let bytes = to_bytes(body, 64 * 1024).await.unwrap();
    let me: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(me["oidcSub"], "sub-alice");
    let sessions = me["sessions"].as_array().expect("sessions array");
    assert_eq!(sessions.len(), 2, "alice has exactly her two own sessions");
    for s in sessions {
        assert!(s["id"].is_string(), "session carries an id");
        assert!(s.get("userAgent").is_some(), "session carries userAgent");
        assert!(s.get("lastSeen").is_some(), "session carries lastSeen");
    }
    let current_entry = sessions
        .iter()
        .find(|s| s["id"] == current.to_string())
        .expect("current session present");
    assert_eq!(
        current_entry["current"], true,
        "the request's own session is identifiable as current"
    );
    let other_entry = sessions
        .iter()
        .find(|s| s["id"] == other.to_string())
        .expect("other session present");
    assert_eq!(other_entry["current"], false);
    assert_eq!(other_entry["userAgent"], "Firefox/macOS");

    // No / invalid session cookie → 401 (and the row is untouched).
    assert_eq!(
        delete_status(&app, &format!("/api/sessions/{other}"), None).await,
        StatusCode::UNAUTHORIZED,
    );
    assert!(session_exists(&pool, other).await);

    // Another user's session → 404 (don't leak its existence), Bob's row survives.
    assert_eq!(
        delete_status(&app, &format!("/api/sessions/{bob_session}"), Some(current)).await,
        StatusCode::NOT_FOUND,
    );
    assert!(session_exists(&pool, bob_session).await);

    // A nonexistent session id → 404.
    assert_eq!(
        delete_status(
            &app,
            &format!("/api/sessions/{}", Uuid::new_v4()),
            Some(current)
        )
        .await,
        StatusCode::NOT_FOUND,
    );

    // The current session is protected from revoke → 403, and it survives.
    assert_eq!(
        delete_status(&app, &format!("/api/sessions/{current}"), Some(current)).await,
        StatusCode::FORBIDDEN,
    );
    assert!(session_exists(&pool, current).await);

    // A non-current own session → 204, and the row is gone.
    assert_eq!(
        delete_status(&app, &format!("/api/sessions/{other}"), Some(current)).await,
        StatusCode::NO_CONTENT,
    );
    assert!(!session_exists(&pool, other).await);

    // A request bearing the now-revoked cookie loads no user (forced re-login).
    let (status, _) = get_json(&app, "/api/me", Some(other)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
