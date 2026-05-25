//! `LogTransport` — the dev-default transport: it doesn't deliver anything, it
//! logs the message at info level so developers can see outbound mail without a
//! mail server.

use async_trait::async_trait;
use junius_sdk::{EmailMessage, Transport, TransportError};

pub struct LogTransport;

#[async_trait]
impl Transport for LogTransport {
    async fn send(&self, from: &str, msg: &EmailMessage) -> Result<(), TransportError> {
        tracing::info!(
            %from,
            to = ?msg.to,
            subject = %msg.subject,
            attachments = msg.attachments.len(),
            "email (log transport): {}",
            msg.body_text,
        );
        Ok(())
    }
}
