//! Unit-ish test for the hello `SendGreeting` job wiring (M10): the registered
//! handler emails via the gated `email.send` capability, and the manifest's
//! `[storage.buckets.attachments]` generates a typed `Bucket::Attachments`.
//! No container: the handler only touches `resources.email`, so a lazy DB pool
//! (never connected) suffices.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use hello_plugin::{HelloPlugin, SendGreeting};
use junius_sdk::{
    AuditEmitter, Auth, Authz, BucketName, Email, EmailMessage, Groups, Jobs, Plugin, PluginConfig,
    PluginDb, PluginResourceCtx, PluginResources, PluginStorage, SecretStore, Telemetry, Transport,
    TransportError, Users,
};
use sqlx::PgPool;

const CAPS: &[&str] = &["email.send", "job.enqueue", "storage.read", "storage.write"];

#[derive(Default)]
struct CapturingTransport {
    sent: Mutex<Vec<EmailMessage>>,
}

#[async_trait]
impl Transport for CapturingTransport {
    async fn send(&self, _from: &str, msg: &EmailMessage) -> Result<(), TransportError> {
        self.sent.lock().unwrap().push(msg.clone());
        Ok(())
    }
}

#[tokio::test]
async fn send_greeting_job_emails_via_capability() {
    // A lazy pool: the handler never reads the DB, so this never connects.
    let pool = PgPool::connect_lazy("postgres://localhost/unused").unwrap();
    let transport = Arc::new(CapturingTransport::default());
    let email = Email::new(
        Some(transport.clone()),
        Arc::from("no-reply@local"),
        Arc::from(vec!["local".to_string()]),
        "hello",
        CAPS,
    );
    let ctx = PluginResourceCtx {
        config: PluginConfig::empty(),
        telemetry: Telemetry::new("hello"),
        db: PluginDb::new(pool.clone(), "hello"),
        auth: Auth::new(pool.clone()),
        users: Users::new(pool.clone()),
        groups: Groups::new(pool.clone()),
        audit: AuditEmitter::new(pool.clone()),
        authz: Authz::new(pool.clone()),
        email,
        jobs: Jobs::disabled("hello", CAPS),
        storage: PluginStorage::empty("hello", CAPS),
        localizer: {
            let mut b = junius_sdk::LocalizerBuilder::new(junius_sdk::Locale::En);
            <HelloPlugin as junius_sdk::Plugin>::register_i18n(&HelloPlugin::new(), &mut b);
            b.build()
        },
        secrets: SecretStore::default(),
        capabilities: CAPS,
    };
    let resources = PluginResources::from_ctx(&ctx, None);

    // Dispatch the registered handler by its job name.
    let handler = HelloPlugin::new()
        .jobs()
        .into_iter()
        .find(|h| h.name() == "hello.send_greeting")
        .expect("hello registers send_greeting");
    let payload = serde_json::to_value(SendGreeting {
        greeting_id: "00000000-0000-0000-0000-000000000000".to_string(),
        name: "Ada".to_string(),
        body: "hello there".to_string(),
        recipient_email: "ada@example.test".to_string(),
        recipient_locale: None,
    })
    .unwrap();
    handler.dispatch(payload, resources).await.unwrap();

    let sent = transport.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to, vec!["ada@example.test".to_string()]);
    assert_eq!(sent[0].body_text, "hello there");
    // Subject rendered via the i18n catalog (en source). M14 end-to-end.
    assert_eq!(sent[0].subject, "A greeting for Ada");

    // The manifest bucket generated a compile-checked typed variant.
    assert_eq!(
        hello_plugin::buckets::Bucket::Attachments.logical(),
        "attachments"
    );
}

#[tokio::test]
async fn send_greeting_uses_recipient_locale_for_subject() {
    let pool = PgPool::connect_lazy("postgres://localhost/unused").unwrap();
    let transport = Arc::new(CapturingTransport::default());
    let email = Email::new(
        Some(transport.clone()),
        Arc::from("no-reply@local"),
        Arc::from(vec!["local".to_string()]),
        "hello",
        CAPS,
    );
    let ctx = PluginResourceCtx {
        config: PluginConfig::empty(),
        telemetry: Telemetry::new("hello"),
        db: PluginDb::new(pool.clone(), "hello"),
        auth: Auth::new(pool.clone()),
        users: Users::new(pool.clone()),
        groups: Groups::new(pool.clone()),
        audit: AuditEmitter::new(pool.clone()),
        authz: Authz::new(pool.clone()),
        email,
        jobs: Jobs::disabled("hello", CAPS),
        storage: PluginStorage::empty("hello", CAPS),
        localizer: {
            let mut b = junius_sdk::LocalizerBuilder::new(junius_sdk::Locale::En);
            <HelloPlugin as junius_sdk::Plugin>::register_i18n(&HelloPlugin::new(), &mut b);
            b.build()
        },
        secrets: SecretStore::default(),
        capabilities: CAPS,
    };
    let resources = PluginResources::from_ctx(&ctx, None);
    let handler = HelloPlugin::new()
        .jobs()
        .into_iter()
        .find(|h| h.name() == "hello.send_greeting")
        .expect("hello registers send_greeting");

    let payload = serde_json::to_value(SendGreeting {
        greeting_id: "00000000-0000-0000-0000-000000000000".to_string(),
        name: "Anna".to_string(),
        body: "Hallo".to_string(),
        recipient_email: "anna@example.test".to_string(),
        recipient_locale: Some("de".to_string()),
    })
    .unwrap();
    handler.dispatch(payload, resources).await.unwrap();

    let sent = transport.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].subject, "Ein Gruß für Anna");
}
