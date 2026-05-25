//! Resolution of `[config]` values, including secret indirections.
//!
//! A `[config]` value is either a literal (`database_url = "postgres://..."`) or
//! an indirection naming where the real value lives (`oidc_client_secret =
//! "env:OIDC_CLIENT_SECRET"`), so secrets need not be committed to the config
//! file. Recognized indirection schemes are `env:` (and, reserved for later,
//! `vault:`); any other value is a literal.
//!
//! This module is IO-free: [`SecretRef::resolve`] takes a lookup closure so the
//! manifest crate never reads the environment itself. The CLI and host pass a
//! `std::env::var`-backed closure (each with the narrow `disallowed_methods`
//! allow); tests pass a fake map. Keeping resolution here — consumed by both the
//! migration runner and the host — is what guarantees they agree on values.

use std::collections::BTreeMap;
use std::str::FromStr;

use crate::error::SecretError;
use crate::infra_config::{
    AuditConfig, EmailConfig, JobsConfig, OtelConfig, StorageConfig, parse_audit, parse_email,
    parse_job_workers, parse_jobs, parse_otel, parse_storage,
};

/// A parsed `[config]` value: either a literal or an environment indirection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretRef {
    Literal(String),
    Env(String),
}

impl FromStr for SecretRef {
    type Err = SecretError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(var) = s.strip_prefix("env:") {
            if var.is_empty() {
                return Err(SecretError::EmptyTarget);
            }
            Ok(Self::Env(var.to_string()))
        } else if s.starts_with("vault:") {
            // Reserved indirection scheme; resolution deferred (see design §10).
            Err(SecretError::UnsupportedScheme("vault".to_string()))
        } else {
            Ok(Self::Literal(s.to_string()))
        }
    }
}

impl SecretRef {
    /// Resolve to the concrete value. `lookup` reads an environment variable
    /// (returning `None` when unset); literals ignore it.
    pub fn resolve(&self, lookup: &impl Fn(&str) -> Option<String>) -> Result<String, SecretError> {
        match self {
            Self::Literal(value) => Ok(value.clone()),
            Self::Env(var) => lookup(var).ok_or_else(|| SecretError::EnvMissing(var.clone())),
        }
    }
}

/// The M06 subset of `[config]`, with every value resolved to a concrete string.
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub database_url: String,
    pub oidc_issuer: String,
    pub oidc_client_id: String,
    pub oidc_client_secret: String,
    pub session_encryption_key: String,
    pub role_password_secret: String,
    /// Optional `bind_addr` override; the host falls back to its default.
    pub bind_addr: Option<String>,
    /// Optional browser-facing OIDC callback URL. When set, the host registers
    /// this as the `redirect_uri` instead of deriving one from `bind_addr`. In
    /// `junius dev` it points at the Vite origin (`:5173`) so the whole login
    /// round-trip — including the callback — flows through the single dev origin.
    pub oidc_redirect_url: Option<String>,

    // --- M10 infra sections (all optional; absent = capability unconfigured) ---
    /// `[config.jobs]` — message-broker connection for the job queue.
    pub jobs: Option<JobsConfig>,
    /// `[config].job_workers` — worker concurrency (default 4).
    pub job_workers: u16,
    /// `[config.email]` — outbound email transport + sender policy.
    pub email: Option<EmailConfig>,
    /// `[config.otel]` — OpenTelemetry export (default disabled).
    pub otel: OtelConfig,
    /// `[config.audit]` — audit retention policy (default 365 days).
    pub audit: AuditConfig,
    /// `[config.storage]` — physical buckets + logical→physical mapping.
    pub storage: Option<StorageConfig>,
}

fn required(
    raw: &BTreeMap<String, toml::Value>,
    key: &str,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<String, SecretError> {
    let value = raw
        .get(key)
        .ok_or_else(|| SecretError::MissingKey(key.to_string()))?;
    let text = value
        .as_str()
        .ok_or_else(|| SecretError::NotAString(key.to_string()))?;
    SecretRef::from_str(text)?.resolve(lookup)
}

fn optional(
    raw: &BTreeMap<String, toml::Value>,
    key: &str,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<Option<String>, SecretError> {
    match raw.get(key) {
        None => Ok(None),
        Some(value) => {
            let text = value
                .as_str()
                .ok_or_else(|| SecretError::NotAString(key.to_string()))?;
            Ok(Some(SecretRef::from_str(text)?.resolve(lookup)?))
        }
    }
}

/// Resolve the `[config]` table into the typed [`ResolvedConfig`]. `lookup`
/// reads environment variables for `env:` indirections.
pub fn resolve_config(
    raw: &BTreeMap<String, toml::Value>,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<ResolvedConfig, SecretError> {
    // The M10 section parsers navigate nested sub-tables, so work over a
    // `toml::value::Table` view of the `[config]` map.
    let cfg: toml::value::Table = raw.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let dyn_lookup: &dyn Fn(&str) -> Option<String> = lookup;

    Ok(ResolvedConfig {
        database_url: required(raw, "database_url", lookup)?,
        oidc_issuer: required(raw, "oidc_issuer", lookup)?,
        oidc_client_id: required(raw, "oidc_client_id", lookup)?,
        oidc_client_secret: required(raw, "oidc_client_secret", lookup)?,
        session_encryption_key: required(raw, "session_encryption_key", lookup)?,
        role_password_secret: required(raw, "role_password_secret", lookup)?,
        bind_addr: optional(raw, "bind_addr", lookup)?,
        oidc_redirect_url: optional(raw, "oidc_redirect_url", lookup)?,
        jobs: parse_jobs(&cfg, dyn_lookup)?,
        job_workers: parse_job_workers(&cfg),
        email: parse_email(&cfg, dyn_lookup)?,
        otel: parse_otel(&cfg),
        audit: parse_audit(&cfg),
        storage: parse_storage(&cfg, dyn_lookup)?,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests panic on unexpected failures")]
mod tests {
    use super::*;

    fn no_env() -> impl Fn(&str) -> Option<String> {
        |_| None
    }

    #[test]
    fn parses_literal_and_env() {
        assert_eq!(
            "postgres://x".parse::<SecretRef>().unwrap(),
            SecretRef::Literal("postgres://x".to_string())
        );
        assert_eq!(
            "env:FOO".parse::<SecretRef>().unwrap(),
            SecretRef::Env("FOO".to_string())
        );
    }

    #[test]
    fn rejects_empty_env_and_reserved_vault() {
        assert!(matches!(
            "env:".parse::<SecretRef>(),
            Err(SecretError::EmptyTarget)
        ));
        assert!(matches!(
            "vault:secret/x".parse::<SecretRef>(),
            Err(SecretError::UnsupportedScheme(_))
        ));
    }

    #[test]
    fn resolves_env_via_lookup() {
        let env = |k: &str| (k == "FOO").then(|| "bar".to_string());
        assert_eq!(
            SecretRef::Env("FOO".to_string()).resolve(&env).unwrap(),
            "bar"
        );
        assert!(matches!(
            SecretRef::Env("MISSING".to_string()).resolve(&env),
            Err(SecretError::EnvMissing(_))
        ));
    }

    #[test]
    fn resolve_config_requires_all_keys() {
        let mut raw = BTreeMap::new();
        raw.insert("database_url".into(), toml::Value::from("postgres://x"));
        let err = resolve_config(&raw, &no_env()).unwrap_err();
        assert!(matches!(err, SecretError::MissingKey(k) if k == "oidc_issuer"));
    }

    #[test]
    fn resolve_config_mixes_literals_and_env() {
        let env = |k: &str| (k == "CLIENT_SECRET").then(|| "shh".to_string());
        let mut raw = BTreeMap::new();
        for (k, v) in [
            ("database_url", "postgres://x"),
            ("oidc_issuer", "http://localhost:9000/"),
            ("oidc_client_id", "platform"),
            ("oidc_client_secret", "env:CLIENT_SECRET"),
            ("session_encryption_key", "literalkey"),
            ("role_password_secret", "rolesecret"),
        ] {
            raw.insert(k.into(), toml::Value::from(v));
        }
        let cfg = resolve_config(&raw, &env).unwrap();
        assert_eq!(cfg.oidc_client_secret, "shh");
        assert_eq!(cfg.database_url, "postgres://x");
        assert_eq!(cfg.bind_addr, None);
        assert_eq!(cfg.oidc_redirect_url, None);
    }
}
