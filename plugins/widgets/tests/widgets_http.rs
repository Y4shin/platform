//! Smoke test for the widgets plugin's only backend surface: `GET /ping`. The
//! plugin is UI-only (its real contribution is the `VenuePicker` component), so
//! there is no RPC or DB to exercise here.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::body::{Body, to_bytes};
use http::{Request, StatusCode};
use junius_sdk::Plugin;
use tower::ServiceExt;
use widgets_plugin::WidgetsPlugin;

#[tokio::test]
async fn http_ping_works() {
    let app = WidgetsPlugin::new().routes();
    let response = app
        .oneshot(Request::get("/ping").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024).await.unwrap();
    assert_eq!(&bytes[..], b"pong");
}
