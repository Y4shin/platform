//! SMTP transports via lettre: `build_smtp` (TLS relay + optional auth) and
//! `build_mailpit` (plaintext to the dev mailpit container). Both wrap an
//! `AsyncSmtpTransport` behind the SDK's [`Transport`](junius_sdk::Transport).

use std::sync::Arc;

use async_trait::async_trait;
use junius_manifest::EmailConfig;
use junius_sdk::{EmailMessage, Transport, TransportError};
use lettre::AsyncTransport as _;
use lettre::message::header::ContentType;
use lettre::message::{Attachment as LettreAttachment, Mailbox, Message, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, Tokio1Executor};

/// Plaintext SMTP to mailpit (`[config.email.mailpit]`). No TLS, no auth — dev only.
pub fn build_mailpit(cfg: &EmailConfig) -> anyhow::Result<Arc<dyn Transport>> {
    let m = cfg.mailpit.as_ref().ok_or_else(|| {
        anyhow::anyhow!("email transport `mailpit` requires [config.email.mailpit]")
    })?;
    let inner = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&m.host)
        .port(m.port)
        .build();
    Ok(Arc::new(SmtpTransport { inner }))
}

/// TLS SMTP relay (`[config.email.smtp]`) with optional username/password auth.
pub fn build_smtp(cfg: &EmailConfig) -> anyhow::Result<Arc<dyn Transport>> {
    let s = cfg
        .smtp
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("email transport `smtp` requires [config.email.smtp]"))?;
    let mut builder = AsyncSmtpTransport::<Tokio1Executor>::relay(&s.host)?.port(s.port);
    if let (Some(user), Some(pass)) = (&s.username, &s.password) {
        builder = builder.credentials(Credentials::new(user.clone(), pass.clone()));
    }
    Ok(Arc::new(SmtpTransport {
        inner: builder.build(),
    }))
}

struct SmtpTransport {
    inner: AsyncSmtpTransport<Tokio1Executor>,
}

#[async_trait]
impl Transport for SmtpTransport {
    async fn send(&self, from: &str, msg: &EmailMessage) -> Result<(), TransportError> {
        let message = build_message(from, msg)?;
        self.inner
            .send(message)
            .await
            .map_err(|e| TransportError::Delivery(e.to_string()))?;
        Ok(())
    }
}

/// Render an [`EmailMessage`] into a lettre [`Message`], choosing a plain /
/// alternative / mixed structure based on whether HTML + attachments are present.
fn build_message(from: &str, msg: &EmailMessage) -> Result<Message, TransportError> {
    let addr = |s: &str| -> Result<Mailbox, TransportError> {
        s.parse::<Mailbox>()
            .map_err(|e| TransportError::Delivery(format!("invalid email address `{s}`: {e}")))
    };

    let mut builder = Message::builder()
        .from(addr(from)?)
        .subject(msg.subject.clone());
    for to in &msg.to {
        builder = builder.to(addr(to)?);
    }
    if let Some(reply_to) = &msg.reply_to {
        builder = builder.reply_to(addr(reply_to)?);
    }

    let result = if msg.attachments.is_empty() {
        match &msg.body_html {
            Some(html) => builder.multipart(MultiPart::alternative_plain_html(
                msg.body_text.clone(),
                html.clone(),
            )),
            None => builder.singlepart(SinglePart::plain(msg.body_text.clone())),
        }
    } else {
        // `MultiPart::mixed()` yields a builder whose first part finalizes it into
        // a `MultiPart`; further parts append via `MultiPart::singlepart`.
        let mut mixed = match &msg.body_html {
            Some(html) => MultiPart::mixed().multipart(MultiPart::alternative_plain_html(
                msg.body_text.clone(),
                html.clone(),
            )),
            None => MultiPart::mixed().singlepart(SinglePart::plain(msg.body_text.clone())),
        };
        for att in &msg.attachments {
            let content_type = ContentType::parse(&att.content_type).map_err(|e| {
                TransportError::Delivery(format!("bad content-type `{}`: {e}", att.content_type))
            })?;
            mixed = mixed.singlepart(
                LettreAttachment::new(att.filename.clone()).body(att.content.clone(), content_type),
            );
        }
        builder.multipart(mixed)
    };

    result.map_err(|e| TransportError::Delivery(format!("building email: {e}")))
}
