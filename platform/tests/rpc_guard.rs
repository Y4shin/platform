//! The host RPC guard enforces each method's `(platform.v1.requires)` from the
//! generated table, before dispatch. Pure middleware test — the downstream
//! handler is a stub, so no DB is needed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashSet;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware::from_fn;
use axum::routing::post;
use axum::{Extension, Router};
use junius_sdk::{GroupId, Membership, Role, RoleId, User, UserId};
use platform::rpc_guard::require_permissions;
use tower::ServiceExt;
use uuid::Uuid;

fn user_with(perms: &[&str]) -> User {
    User {
        id: UserId(Uuid::nil()),
        email: "u@example.com".into(),
        display_name: "U".into(),
        locale: None,
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

fn app(user: Option<User>) -> Router {
    let mut app = Router::new()
        .route(
            "/hello.v1.HelloService/CreateGreeting",
            post(|| async { "ok" }),
        )
        .route("/hello.v1.HelloService/Greet", post(|| async { "ok" }))
        .layer(from_fn(require_permissions));
    if let Some(u) = user {
        app = app.layer(Extension(u));
    }
    app
}

async fn status(user: Option<User>, path: &str) -> StatusCode {
    app(user)
        .oneshot(Request::post(path).body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn guard_enforces_required_permissions() {
    // CreateGreeting requires hello:read + hello:write.
    let create = "/hello.v1.HelloService/CreateGreeting";

    // No caller → 401.
    assert_eq!(status(None, create).await, StatusCode::UNAUTHORIZED);

    // Holds read but not write → 403.
    assert_eq!(
        status(Some(user_with(&["hello:read"])), create).await,
        StatusCode::FORBIDDEN
    );

    // Holds both → passes the guard (stub handler returns 200).
    assert_eq!(
        status(Some(user_with(&["hello:read", "hello:write"])), create).await,
        StatusCode::OK
    );

    // Greet is unannotated → no requirement, allowed even anonymously.
    assert_eq!(
        status(None, "/hello.v1.HelloService/Greet").await,
        StatusCode::OK
    );
}
