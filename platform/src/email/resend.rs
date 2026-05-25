//! Resend transport — delivers via the Resend HTTP API
//! (`POST https://api.resend.com/emails`) using the shared reqwest client.

use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine as _;
use junius_manifest::EmailConfig;
use junius_sdk::{EmailMessage, Transport, TransportError};

const RESEND_ENDPOINT: &str = "https://api.resend.com/emails";

pub fn build_resend(cfg: &EmailConfig) -> anyhow::Result<Arc<dyn Transport>> {
    let r = cfg.resend.as_ref().ok_or_else(|| {
        anyhow::anyhow!("email transport `resend` requires [config.email.resend]")
    })?;
    Ok(Arc::new(ResendTransport {
        api_key: r.api_key.clone(),
        client: reqwest::Client::new(),
    }))
}

struct ResendTransport {
    api_key: String,
    client: reqwest::Client,
}

#[async_trait]
impl Transport for ResendTransport {
    async fn send(&self, from: &str, msg: &EmailMessage) -> Result<(), TransportError> {
        let mut body = serde_json::json!({
            "from": from,
            "to": msg.to,
            "subject": msg.subject,
            "text": msg.body_text,
        });
        if let Some(html) = &msg.body_html {
            body["html"] = serde_json::Value::String(html.clone());
        }
        if let Some(reply_to) = &msg.reply_to {
            body["reply_to"] = serde_json::Value::String(reply_to.clone());
        }
        if !msg.attachments.is_empty() {
            let attachments: Vec<serde_json::Value> = msg
                .attachments
                .iter()
                .map(|a| {
                    serde_json::json!({
                        "filename": a.filename,
                        "content": base64::engine::general_purpose::STANDARD.encode(&a.content),
                    })
                })
                .collect();
            body["attachments"] = serde_json::Value::Array(attachments);
        }

        let resp = self
            .client
            .post(RESEND_ENDPOINT)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| TransportError::Delivery(format!("resend request failed: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(TransportError::Delivery(format!(
                "resend returned {status}: {text}"
            )));
        }
        Ok(())
    }
}
