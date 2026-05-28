//! Unit-ish test for the events `SendSignupConfirmation` job wiring (M13 Stage 8):
//! the registered handler emails the attendee via the gated `email.send`
//! capability. No container — the handler only touches `resources.email`, so a
//! lazy DB pool (never connected) suffices.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use events_plugin::{EventsPlugin, SendSignupConfirmation};
use junius_sdk::{
    AuditEmitter, Auth, Authz, Email, EmailMessage, Groups, Jobs, PlatformAdminApi, Plugin,
    PluginConfig, PluginDb, PluginResourceCtx, PluginResources, PluginStorage, SecretStore,
    Telemetry, Transport, TransportError, Users,
};
use sqlx::PgPool;

const CAPS: &[&str] = &["email.send", "job.enqueue"];

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
async fn send_signup_confirmation_emails_via_capability() {
    // A lazy pool: the handler never reads the DB, so this never connects.
    let pool = PgPool::connect_lazy("postgres://localhost/unused").unwrap();
    let transport = Arc::new(CapturingTransport::default());
    let email = Email::new(
        Some(transport.clone()),
        Arc::from("no-reply@local"),
        Arc::from(vec!["local".to_string()]),
        "events",
        CAPS,
    );
    let ctx = PluginResourceCtx {
        config: PluginConfig::empty(),
        telemetry: Telemetry::new("events"),
        db: PluginDb::new(pool.clone(), "events"),
        auth: Auth::new(pool.clone()),
        users: Users::new(pool.clone()),
        groups: Groups::new(pool.clone()),
        audit: AuditEmitter::new(pool.clone()),
        authz: Authz::new(pool.clone()),
        email,
        jobs: Jobs::disabled("events", CAPS),
        storage: PluginStorage::empty("events", CAPS),
        platform_admin: PlatformAdminApi::new(
            pool.clone(),
            AuditEmitter::new(pool.clone()),
            "events",
            CAPS,
            std::sync::Arc::new(Vec::new()),
            false,
        ),
        localizer: {
            let mut b = junius_sdk::LocalizerBuilder::new(junius_sdk::Locale::En);
            <EventsPlugin as junius_sdk::Plugin>::register_i18n(&EventsPlugin::new(), &mut b);
            b.build()
        },
        secrets: SecretStore::default(),
        capabilities: CAPS,
    };
    let resources = PluginResources::from_ctx(&ctx, None);

    let handler = EventsPlugin::new()
        .jobs()
        .into_iter()
        .find(|h| h.name() == "events.send_signup_confirmation")
        .expect("events registers send_signup_confirmation");
    let payload = serde_json::to_value(SendSignupConfirmation {
        signup_id: "00000000-0000-0000-0000-000000000000".to_string(),
        event_title: "Launch Party".to_string(),
        recipient_email: "ada@example.test".to_string(),
        recipient_name: "Ada".to_string(),
        recipient_locale: None,
    })
    .unwrap();
    handler.dispatch(payload, resources).await.unwrap();

    let sent = transport.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to, vec!["ada@example.test".to_string()]);
    assert_eq!(sent[0].subject, "You're signed up: Launch Party");
    assert!(sent[0].body_text.contains("Hi Ada,"));
    assert!(sent[0].body_text.contains("Launch Party"));
}

#[tokio::test]
async fn send_signup_confirmation_uses_recipient_locale_for_subject_and_body() {
    let pool = PgPool::connect_lazy("postgres://localhost/unused").unwrap();
    let transport = Arc::new(CapturingTransport::default());
    let email = Email::new(
        Some(transport.clone()),
        Arc::from("no-reply@local"),
        Arc::from(vec!["local".to_string()]),
        "events",
        CAPS,
    );
    let ctx = PluginResourceCtx {
        config: PluginConfig::empty(),
        telemetry: Telemetry::new("events"),
        db: PluginDb::new(pool.clone(), "events"),
        auth: Auth::new(pool.clone()),
        users: Users::new(pool.clone()),
        groups: Groups::new(pool.clone()),
        audit: AuditEmitter::new(pool.clone()),
        authz: Authz::new(pool.clone()),
        email,
        jobs: Jobs::disabled("events", CAPS),
        storage: PluginStorage::empty("events", CAPS),
        platform_admin: PlatformAdminApi::new(
            pool.clone(),
            AuditEmitter::new(pool.clone()),
            "events",
            CAPS,
            std::sync::Arc::new(Vec::new()),
            false,
        ),
        localizer: {
            let mut b = junius_sdk::LocalizerBuilder::new(junius_sdk::Locale::En);
            <EventsPlugin as junius_sdk::Plugin>::register_i18n(&EventsPlugin::new(), &mut b);
            b.build()
        },
        secrets: SecretStore::default(),
        capabilities: CAPS,
    };
    let resources = PluginResources::from_ctx(&ctx, None);
    let handler = EventsPlugin::new()
        .jobs()
        .into_iter()
        .find(|h| h.name() == "events.send_signup_confirmation")
        .expect("events registers send_signup_confirmation");

    let payload = serde_json::to_value(SendSignupConfirmation {
        signup_id: "00000000-0000-0000-0000-000000000000".to_string(),
        event_title: "Sommerfest".to_string(),
        recipient_email: "anna@example.test".to_string(),
        recipient_name: "Anna".to_string(),
        recipient_locale: Some("de".to_string()),
    })
    .unwrap();
    handler.dispatch(payload, resources).await.unwrap();

    let sent = transport.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].subject, "Du bist angemeldet: Sommerfest");
    assert!(
        sent[0].body_text.contains("Hallo Anna,"),
        "body should greet in German, got: {}",
        sent[0].body_text
    );
    assert!(sent[0].body_text.contains("„Sommerfest“"));
}

#[tokio::test]
async fn send_signup_confirmation_uses_locale_fallback_for_anonymous_guest() {
    // Guest with empty `recipient_name` + de locale → "Hallo zusammen,"
    // (the locale-specific fallback from `events.signup.email.recipient_fallback`).
    let pool = PgPool::connect_lazy("postgres://localhost/unused").unwrap();
    let transport = Arc::new(CapturingTransport::default());
    let email = Email::new(
        Some(transport.clone()),
        Arc::from("no-reply@local"),
        Arc::from(vec!["local".to_string()]),
        "events",
        CAPS,
    );
    let ctx = PluginResourceCtx {
        config: PluginConfig::empty(),
        telemetry: Telemetry::new("events"),
        db: PluginDb::new(pool.clone(), "events"),
        auth: Auth::new(pool.clone()),
        users: Users::new(pool.clone()),
        groups: Groups::new(pool.clone()),
        audit: AuditEmitter::new(pool.clone()),
        authz: Authz::new(pool.clone()),
        email,
        jobs: Jobs::disabled("events", CAPS),
        storage: PluginStorage::empty("events", CAPS),
        platform_admin: PlatformAdminApi::new(
            pool.clone(),
            AuditEmitter::new(pool.clone()),
            "events",
            CAPS,
            std::sync::Arc::new(Vec::new()),
            false,
        ),
        localizer: {
            let mut b = junius_sdk::LocalizerBuilder::new(junius_sdk::Locale::En);
            <EventsPlugin as junius_sdk::Plugin>::register_i18n(&EventsPlugin::new(), &mut b);
            b.build()
        },
        secrets: SecretStore::default(),
        capabilities: CAPS,
    };
    let resources = PluginResources::from_ctx(&ctx, None);
    let handler = EventsPlugin::new()
        .jobs()
        .into_iter()
        .find(|h| h.name() == "events.send_signup_confirmation")
        .expect("events registers send_signup_confirmation");

    let payload = serde_json::to_value(SendSignupConfirmation {
        signup_id: "00000000-0000-0000-0000-000000000000".to_string(),
        event_title: "Sommerfest".to_string(),
        recipient_email: "guest@example.test".to_string(),
        recipient_name: String::new(),
        recipient_locale: Some("de".to_string()),
    })
    .unwrap();
    handler.dispatch(payload, resources).await.unwrap();

    let sent = transport.sent.lock().unwrap();
    assert!(sent[0].body_text.starts_with("Hallo zusammen,"));
}
