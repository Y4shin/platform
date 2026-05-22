//! In-process tests for the hello plugin's RPC surface. The host nests
//! `Plugin::rpc_routes()` under `/rpc`, so we add the same prefix in the
//! test to mirror real URLs.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::Router;
use axum::body::{Body, to_bytes};
use hello_plugin::{GreetRequest, GreetResponse, HelloPlugin};
use http::{Request, StatusCode, header};
use junius_sdk::{Plugin, PluginConfig, PluginResources, Telemetry};
use prost::Message;
use tower::ServiceExt;

fn stub_resources() -> PluginResources {
    PluginResources::new(PluginConfig::empty(), Telemetry::new("hello"))
}

fn rpc_app() -> Router {
    Router::new().nest("/rpc", HelloPlugin::new().rpc_routes(stub_resources()))
}

#[tokio::test]
async fn greet_binary_round_trip() {
    let app = rpc_app();
    let body = GreetRequest {
        name: "alice".to_string(),
    }
    .encode_to_vec();

    let response = app
        .oneshot(
            Request::post("/rpc/hello.v1.HelloService/Greet")
                .header(header::CONTENT_TYPE, "application/proto")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
    let decoded = GreetResponse::decode(&bytes[..]).unwrap();
    assert_eq!(decoded.message, "Hello, alice!");
}

#[tokio::test]
async fn greet_json_round_trip() {
    let app = rpc_app();

    let response = app
        .oneshot(
            Request::post("/rpc/hello.v1.HelloService/Greet")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"name":"bob"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["message"], "Hello, bob!");
}

#[tokio::test]
async fn greet_empty_name_defaults_to_world() {
    let app = rpc_app();
    let body = GreetRequest {
        name: String::new(),
    }
    .encode_to_vec();

    let response = app
        .oneshot(
            Request::post("/rpc/hello.v1.HelloService/Greet")
                .header(header::CONTENT_TYPE, "application/proto")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
    let decoded = GreetResponse::decode(&bytes[..]).unwrap();
    assert_eq!(decoded.message, "Hello, world!");
}

#[tokio::test]
async fn unknown_rpc_method_404s() {
    let app = rpc_app();
    let response = app
        .oneshot(
            Request::post("/rpc/hello.v1.HelloService/DoesNotExist")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn http_ping_still_works() {
    // RPC surface is independent of the HTTP surface; sanity-check the M03
    // /ping handler hasn't regressed.
    let app = HelloPlugin::new().routes(stub_resources());
    let response = app
        .oneshot(Request::get("/ping").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024).await.unwrap();
    assert_eq!(&bytes[..], b"pong");
}
