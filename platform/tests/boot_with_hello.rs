//! End-to-end integration: load the sync-generated plugin registry and
//! confirm the hello plugin's `/h/hello/ping` route is reachable through the
//! composed host server.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::body::{Body, to_bytes};
use hello_plugin::{GreetRequest, GreetResponse, HelloPlugin};
use http::{Request, header};
use junius_sdk::Plugin;
use platform::server;
use prost::Message;
use tower::ServiceExt;

fn hello_registry() -> Vec<Box<dyn Plugin>> {
    vec![Box::new(HelloPlugin::new())]
}

#[tokio::test]
async fn hello_plugin_mount_serves_ping() {
    let plugins = hello_registry();
    let app = server::build_app(&plugins);

    let response = app
        .oneshot(Request::get("/h/hello/ping").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = to_bytes(response.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"pong");
}

#[tokio::test]
async fn root_still_served_with_hello_mounted() {
    let plugins = hello_registry();
    let app = server::build_app(&plugins);

    let response = app
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = to_bytes(response.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"hello, platform");
}

#[tokio::test]
async fn outside_hello_prefix_404s() {
    let plugins = hello_registry();
    let app = server::build_app(&plugins);

    let response = app
        .oneshot(Request::get("/h/other/ping").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn full_server_run_against_hello() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local = listener.local_addr().unwrap();

    let app = server::build_app(&hello_registry());
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });

    let body = reqwest::Client::new()
        .get(format!("http://{local}/h/hello/ping"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(body, "pong");

    let _ = shutdown_tx.send(());
    tokio::time::timeout(std::time::Duration::from_secs(2), handle)
        .await
        .expect("server did not shut down within 2s")
        .unwrap();
}

#[tokio::test]
async fn rpc_route_mounted_under_slash_rpc() {
    let plugins = hello_registry();
    let app = server::build_app(&plugins);
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

    assert_eq!(response.status(), 200);
    let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
    let decoded = GreetResponse::decode(&bytes[..]).unwrap();
    assert_eq!(decoded.message, "Hello, alice!");
}

#[tokio::test]
async fn rpc_route_via_real_bind_and_reqwest_json() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local = listener.local_addr().unwrap();

    let app = server::build_app(&hello_registry());
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });

    let body = reqwest::Client::new()
        .post(format!("http://{local}/rpc/hello.v1.HelloService/Greet"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(r#"{"name":"alice"}"#)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["message"], "Hello, alice!");

    let _ = shutdown_tx.send(());
    tokio::time::timeout(std::time::Duration::from_secs(2), handle)
        .await
        .expect("server did not shut down within 2s")
        .unwrap();
}

#[tokio::test]
async fn missing_plugin_rpc_route_404s() {
    let plugins = hello_registry();
    let app = server::build_app(&plugins);

    let response = app
        .oneshot(
            Request::post("/rpc/other.v1.OtherService/DoesNotExist")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}
