//! Integration tests for the host server.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;

use axum::body::{Body, to_bytes};
use http::Request;
use platform::config::HostConfig;
use platform::server;
use tower::ServiceExt;

#[tokio::test]
async fn root_returns_headless_404() {
    // M23 Stage 1: with `embed-frontend` off, browser routes (including `/`)
    // return a structured 404 so a misrouted request is obvious. The SPA
    // is served by the M23 SSR FE container, not by juniusd.
    let app = server::build_app(&[]);

    let response = app
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
    let body = to_bytes(response.into_body(), 4096).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["code"], "FRONTEND_NOT_EMBEDDED");
    assert_eq!(parsed["path"], "/");
}

#[tokio::test]
async fn unknown_route_404s() {
    let app = server::build_app(&[]);

    let response = app
        .oneshot(Request::get("/nope").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

#[test]
fn host_config_default_binds_localhost_18080() {
    let cfg = HostConfig::default();
    assert_eq!(cfg.bind_addr, SocketAddr::from(([127, 0, 0, 1], 18080)));
}

#[tokio::test]
async fn server_run_binds_and_serves() {
    // Bind to an ephemeral port manually so we can discover the assigned port.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local = listener.local_addr().unwrap();

    let app = server::build_app(&[]);
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });

    let response = reqwest::Client::new()
        .get(format!("http://{local}/"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    let parsed: serde_json::Value = response.json().await.unwrap();
    assert_eq!(parsed["code"], "FRONTEND_NOT_EMBEDDED");

    let _ = shutdown_tx.send(());
    tokio::time::timeout(std::time::Duration::from_secs(2), handle)
        .await
        .expect("server did not shut down within 2s")
        .unwrap();
}
