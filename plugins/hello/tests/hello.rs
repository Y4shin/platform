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
fn generated_config_applies_declared_default() {
    use hello_plugin::Config;
    use junius_sdk::PluginConfig;

    let cfg = Config::load(&PluginConfig::empty()).unwrap();
    assert_eq!(cfg.greeting, "Hello");
}

#[test]
fn generated_config_reads_override() {
    use hello_plugin::Config;
    use junius_sdk::PluginConfig;

    let mut table = toml::Table::new();
    table.insert(
        "greeting".to_string(),
        toml::Value::String("Hi".to_string()),
    );
    let cfg = Config::load(&PluginConfig::from_table(table)).unwrap();
    assert_eq!(cfg.greeting, "Hi");
}

#[test]
fn generated_secrets_accessor_reads_declared_secret() {
    use hello_plugin::Secrets;
    use junius_sdk::SecretStore;

    let store = SecretStore::from_pairs([("api_key".to_string(), "xyz".to_string())]);
    let secrets = Secrets::new(&store);
    assert_eq!(secrets.api_key().expose(), "xyz");
    // `secrets.undeclared()` would be a compile error — no method is generated.
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
    let perms: Vec<&str> = m.permissions.iter().map(|p| p.name).collect();
    assert_eq!(perms, ["hello:read", "hello:share", "hello:write"]);
    assert!(m.dependencies.is_empty());
}
