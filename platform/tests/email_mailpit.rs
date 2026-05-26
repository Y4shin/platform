//! Integration test for the SMTP email path against an ephemeral mailpit
//! container: build the `mailpit` transport, send through the SDK `Email` handle,
//! and assert via mailpit's HTTP API that the message arrived. Skips cleanly
//! without Docker.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use junius_manifest::{EmailConfig, MailpitConfig};
use junius_sdk::{Email, EmailMessage};
use platform::email::build_transport;
use testcontainers_modules::testcontainers::GenericImage;
use testcontainers_modules::testcontainers::core::ports::IntoContainerPort;
use testcontainers_modules::testcontainers::core::wait::{HttpWaitStrategy, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;

#[tokio::test]
async fn mailpit_smtp_round_trip() {
    let image = GenericImage::new("axllent/mailpit", "latest")
        .with_exposed_port(1025.tcp())
        .with_exposed_port(8025.tcp())
        .with_wait_for(WaitFor::http(
            HttpWaitStrategy::new("/")
                .with_port(8025.tcp())
                .with_expected_status_code(200u16),
        ));
    let node = match image.start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping mailpit_smtp_round_trip: Docker unavailable ({e})");
            return;
        }
    };
    let smtp_port = node.get_host_port_ipv4(1025).await.unwrap();
    let http_port = node.get_host_port_ipv4(8025).await.unwrap();

    let cfg = EmailConfig {
        transport: "mailpit".to_string(),
        from_default: "no-reply@local".to_string(),
        allowed_sender_domains: vec!["local".to_string()],
        resend: None,
        smtp: None,
        mailpit: Some(MailpitConfig {
            host: "127.0.0.1".to_string(),
            port: smtp_port,
        }),
    };
    let transport = build_transport(&cfg).unwrap();
    let email = Email::new(
        Some(transport),
        Arc::from("no-reply@local"),
        Arc::from(vec!["local".to_string()]),
        "events",
        &["email.send"],
    );

    email
        .send(EmailMessage {
            to: vec!["alice@local".to_string()],
            subject: "Greetings from Junius".to_string(),
            body_text: "hello from the mailpit test".to_string(),
            ..Default::default()
        })
        .await
        .unwrap();

    // Poll mailpit's HTTP API until the message is indexed (SMTP delivery is
    // synchronous, but allow a brief settle).
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{http_port}/api/v1/messages");
    let mut found = None;
    for _ in 0..30 {
        if let Ok(resp) = client.get(&url).send().await {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if body["total"].as_i64().unwrap_or(0) >= 1 {
                    found = Some(body);
                    break;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    let body = found.expect("message should arrive in mailpit");
    let first = &body["messages"][0];
    assert_eq!(first["Subject"], "Greetings from Junius");
    assert_eq!(first["To"][0]["Address"], "alice@local");
    assert_eq!(first["From"]["Address"], "no-reply@local");
}
