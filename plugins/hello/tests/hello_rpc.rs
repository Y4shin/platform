//! In-process tests for the hello plugin's RPC surface. The host folds each
//! plugin's `register_rpc` into one connectrpc router under `/rpc`; here we build
//! a one-plugin router the same way. We exercise the Connect JSON codec
//! end-to-end; protocol-level correctness (binary framing, streaming,
//! compression) is `connectrpc`'s own conformance responsibility.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::Router;
use axum::body::{Body, to_bytes};
use hello_plugin::HelloPlugin;
use http::{Request, StatusCode, header};
use junius_sdk::Plugin;
use tower::ServiceExt;

fn rpc_app() -> Router {
    let connect = HelloPlugin::new().register_rpc(connectrpc::Router::new());
    Router::new().nest("/rpc", connect.into_axum_router())
}

async fn greet_json(name_json_body: &'static str) -> (StatusCode, serde_json::Value) {
    let response = rpc_app()
        .oneshot(
            Request::post("/rpc/hello.v1.HelloService/Greet")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(name_json_body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, value)
}

#[tokio::test]
async fn greet_json_round_trip() {
    let (status, body) = greet_json(r#"{"name":"alice"}"#).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["message"], "Hello, alice!");
}

#[tokio::test]
async fn greet_empty_name_defaults_to_world() {
    let (status, body) = greet_json(r#"{"name":""}"#).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["message"], "Hello, world!");
}

#[tokio::test]
async fn unknown_rpc_method_is_not_ok() {
    let response = rpc_app()
        .oneshot(
            Request::post("/rpc/hello.v1.HelloService/DoesNotExist")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn http_ping_still_works() {
    // RPC surface is independent of the HTTP surface; sanity-check the M03
    // /ping handler hasn't regressed.
    let app = HelloPlugin::new().routes();
    let response = app
        .oneshot(Request::get("/ping").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024).await.unwrap();
    assert_eq!(&bytes[..], b"pong");
}
