//! Host email transports (M10 Stage 4). The SDK defines the vendor-neutral
//! [`Transport`](junius_sdk::Transport) trait; the concrete backends live here:
//!
//! - `log`    — writes the message to the tracing log (dev default / no-op send),
//! - `smtp`   — real SMTP via lettre (TLS relay, optional auth),
//! - mailpit  — plaintext SMTP to the dev mailpit container (also in `smtp`),
//! - `resend` — the Resend HTTP API via reqwest.
//!
//! [`build_transport`] selects one from `[config.email].transport`.

mod log;
mod resend;
mod smtp;

use std::sync::Arc;

use junius_manifest::EmailConfig;
use junius_sdk::Transport;

/// Build the configured email transport. Returns an error for an unknown
/// transport name or a misconfigured selection (e.g. `smtp` without `[email.smtp]`).
pub fn build_transport(cfg: &EmailConfig) -> anyhow::Result<Arc<dyn Transport>> {
    match cfg.transport.as_str() {
        "log" => Ok(Arc::new(log::LogTransport)),
        "mailpit" => smtp::build_mailpit(cfg),
        "smtp" => smtp::build_smtp(cfg),
        "resend" => resend::build_resend(cfg),
        other => {
            anyhow::bail!("unknown email transport `{other}` (expected log|mailpit|smtp|resend)")
        }
    }
}
