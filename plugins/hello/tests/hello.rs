//! In-process tests for the `hello` plugin's `Plugin::routes` output.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::body::{Body, to_bytes};
use hello_plugin::HelloPlugin;
use http::Request;
use junius_sdk::Plugin;
use tower::ServiceExt;

#[tokio::test]
async fn ping_returns_pong() {
    let plugin = HelloPlugin::new();
    let app = plugin.routes();

    let response = app
        .oneshot(Request::get("/ping").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = to_bytes(response.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"pong");
}

#[tokio::test]
async fn unknown_route_404s() {
    let plugin = HelloPlugin::new();
    let app = plugin.routes();

    let response = app
        .oneshot(Request::get("/nope").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

#[test]
fn metadata_reports_the_right_mount() {
    let plugin = HelloPlugin::new();
    let m = plugin.metadata();
    assert_eq!(m.name, "hello");
    assert_eq!(m.display_name, "Hello");
    assert_eq!(m.mount.http_prefix, "/h/hello");
    assert_eq!(m.mount.route_prefix, "/p/hello");
    assert_eq!(m.mount.rpc_prefix, "/rpc/hello");
    assert!(m.permissions.is_empty());
    assert!(m.dependencies.is_empty());
}
