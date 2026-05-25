//! Vendor-neutral email handle (M10 Stage 4). Plugins build an [`EmailMessage`]
//! and call [`Email::send`]; the host supplies the concrete [`Transport`]
//! (lettre SMTP / mailpit / Resend / a log transport) — no transport crate
//! appears in `junius-sdk`. `send` is gated on the `email.send` capability and
//! validates the sender against the deployment's allowed domains.

use std::sync::Arc;

use async_trait::async_trait;

use crate::error::PluginError;
use crate::resources::require_capability;

/// A file attached to an outgoing email.
#[derive(Clone)]
pub struct Attachment {
    pub filename: String,
    /// MIME type, e.g. `application/pdf`.
    pub content_type: String,
    pub content: Vec<u8>,
}

impl std::fmt::Debug for Attachment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Attachment")
            .field("filename", &self.filename)
            .field("content_type", &self.content_type)
            .field("bytes", &self.content.len())
            .finish()
    }
}

/// An outgoing email. `from` is optional — when `None` the deployment's
/// `from_default` is used; when `Some` it must belong to an allowed sender
/// domain (else [`Email::send`] rejects it).
#[derive(Clone, Debug, Default)]
pub struct EmailMessage {
    pub to: Vec<String>,
    pub from: Option<String>,
    pub reply_to: Option<String>,
    pub subject: String,
    pub body_text: String,
    pub body_html: Option<String>,
    pub attachments: Vec<Attachment>,
}

/// Backend trait the host implements per deployment. Receives the already
/// resolved + validated `from`, so impls don't re-derive it.
#[async_trait]
pub trait Transport: Send + Sync {
    async fn send(&self, from: &str, msg: &EmailMessage) -> Result<(), TransportError>;
}

/// Failure delivering an email through a [`Transport`].
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// The transport rejected or failed to deliver the message.
    #[error("email delivery failed: {0}")]
    Delivery(String),
    /// Any other transport-internal failure.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Per-plugin email handle. Cheap to clone. Constructed by the host in
/// `build_ctx` from the deployment's transport + sender policy.
#[derive(Clone)]
pub struct Email {
    transport: Option<Arc<dyn Transport>>,
    from_default: Arc<str>,
    allowed_domains: Arc<[String]>,
    plugin_name: &'static str,
    capabilities: &'static [&'static str],
}

impl std::fmt::Debug for Email {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Email")
            .field("plugin_name", &self.plugin_name)
            .field("from_default", &self.from_default)
            .field("allowed_domains", &self.allowed_domains)
            .field("capabilities", &self.capabilities)
            .field("transport", &self.transport.is_some())
            .finish()
    }
}

impl Email {
    /// Build a per-plugin email handle. `transport` is `None` when the
    /// deployment configured no `[config.email]`.
    #[must_use]
    pub fn new(
        transport: Option<Arc<dyn Transport>>,
        from_default: Arc<str>,
        allowed_domains: Arc<[String]>,
        plugin_name: &'static str,
        capabilities: &'static [&'static str],
    ) -> Self {
        Self {
            transport,
            from_default,
            allowed_domains,
            plugin_name,
            capabilities,
        }
    }

    /// An email handle with no transport configured. Used for caller-less/default
    /// contexts and tests; `send` errors (after the capability check).
    #[must_use]
    pub fn disabled(plugin_name: &'static str, capabilities: &'static [&'static str]) -> Self {
        Self::new(
            None,
            Arc::from(""),
            Arc::from(Vec::<String>::new()),
            plugin_name,
            capabilities,
        )
    }

    /// Send `msg`. Requires the `email.send` capability; resolves/validates the
    /// sender; then delegates to the deployment's transport.
    pub async fn send(&self, msg: EmailMessage) -> Result<(), PluginError> {
        require_capability(self.capabilities, "email.send")?;
        let from = self.resolve_from(&msg)?;
        let transport = self.transport.as_ref().ok_or_else(|| {
            PluginError::External(anyhow::anyhow!(
                "email.send: no email transport configured for this deployment"
            ))
        })?;
        tracing::debug!(plugin = self.plugin_name, %from, recipients = msg.to.len(), "email.send");
        transport
            .send(&from, &msg)
            .await
            .map_err(|e| PluginError::External(anyhow::Error::new(e)))
    }

    /// Resolve the effective sender: the message's `from` (validated against the
    /// allowed domains) or the deployment default.
    fn resolve_from(&self, msg: &EmailMessage) -> Result<String, PluginError> {
        match msg.from.as_deref() {
            None => {
                if self.from_default.is_empty() {
                    Err(PluginError::External(anyhow::anyhow!(
                        "email.send: message has no `from` and no from_default is configured"
                    )))
                } else {
                    Ok(self.from_default.to_string())
                }
            }
            Some(from) => {
                let domain = from.rsplit('@').next().unwrap_or_default();
                if !domain.is_empty() && self.allowed_domains.iter().any(|d| d == domain) {
                    Ok(from.to_string())
                } else {
                    Err(PluginError::PermissionDenied(format!(
                        "email.send: sender domain not allowed for `{from}`"
                    )))
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests panic on unexpected failures")]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct CapturingTransport {
        sent: Mutex<Vec<(String, EmailMessage)>>,
    }

    #[async_trait]
    impl Transport for CapturingTransport {
        async fn send(&self, from: &str, msg: &EmailMessage) -> Result<(), TransportError> {
            self.sent
                .lock()
                .unwrap()
                .push((from.to_string(), msg.clone()));
            Ok(())
        }
    }

    fn email(caps: &'static [&'static str], transport: Option<Arc<dyn Transport>>) -> Email {
        Email::new(
            transport,
            Arc::from("no-reply@local"),
            Arc::from(vec!["local".to_string(), "platform.example".to_string()]),
            "hello",
            caps,
        )
    }

    fn msg() -> EmailMessage {
        EmailMessage {
            to: vec!["alice@local".to_string()],
            subject: "hi".to_string(),
            body_text: "hello".to_string(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn send_requires_capability() {
        let transport = Arc::new(CapturingTransport::default());
        let e = email(&[], Some(transport.clone()));
        assert!(matches!(
            e.send(msg()).await,
            Err(PluginError::CapabilityNotDeclared("email.send"))
        ));
        assert!(transport.sent.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn send_uses_from_default_when_unset() {
        let transport = Arc::new(CapturingTransport::default());
        let e = email(&["email.send"], Some(transport.clone()));
        e.send(msg()).await.unwrap();
        let sent = transport.sent.lock().unwrap();
        assert_eq!(sent[0].0, "no-reply@local");
        assert_eq!(sent[0].1.to, vec!["alice@local".to_string()]);
    }

    #[tokio::test]
    async fn send_accepts_allowed_sender_domain() {
        let transport = Arc::new(CapturingTransport::default());
        let e = email(&["email.send"], Some(transport.clone()));
        let mut m = msg();
        m.from = Some("events@platform.example".to_string());
        e.send(m).await.unwrap();
        assert_eq!(
            transport.sent.lock().unwrap()[0].0,
            "events@platform.example"
        );
    }

    #[tokio::test]
    async fn send_rejects_disallowed_sender_domain() {
        let transport = Arc::new(CapturingTransport::default());
        let e = email(&["email.send"], Some(transport.clone()));
        let mut m = msg();
        m.from = Some("attacker@evil.example".to_string());
        assert!(matches!(
            e.send(m).await,
            Err(PluginError::PermissionDenied(_))
        ));
        assert!(transport.sent.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn send_without_transport_errors() {
        let e = email(&["email.send"], None);
        assert!(e.send(msg()).await.is_err());
    }
}
