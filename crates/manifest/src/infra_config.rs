//! Typed `[config]` sections for the M10 infra capabilities (jobs, storage,
//! email, telemetry, audit). Parsed out of the deployment manifest's `[config]`
//! table into concrete structs, with `env:` secret indirections resolved through
//! the same [`SecretRef`](crate::secrets::SecretRef) lookup the rest of the host
//! uses. All sections are optional so deployments that don't use a capability
//! (and existing test fixtures) parse cleanly.

use std::collections::BTreeMap;
use std::str::FromStr;

use toml::value::Table;

use crate::error::SecretError;
use crate::secrets::SecretRef;

type Lookup<'a> = &'a dyn Fn(&str) -> Option<String>;

/// `[config.jobs]` — the message-broker connection (`RabbitMQ` in v1).
#[derive(Debug, Clone)]
pub struct JobsConfig {
    /// AMQP URL, e.g. `amqp://guest:guest@localhost:5672/%2f` (often `env:`).
    pub amqp_url: String,
}

/// `[config.otel]` — OpenTelemetry export (off by default).
#[derive(Debug, Clone, Default)]
pub struct OtelConfig {
    pub enabled: bool,
    /// OTLP gRPC endpoint, e.g. `http://localhost:4317`.
    pub endpoint: Option<String>,
}

/// `[config.audit]` — audit-log retention policy.
#[derive(Debug, Clone)]
pub struct AuditConfig {
    pub retention_days: i64,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            retention_days: 365,
        }
    }
}

/// `[config.email]` — outbound email transport selection + sender policy.
#[derive(Debug, Clone)]
pub struct EmailConfig {
    /// `"log"` | `"mailpit"` | `"smtp"` | `"resend"`.
    pub transport: String,
    pub from_default: String,
    /// `from` addresses must belong to one of these domains.
    pub allowed_sender_domains: Vec<String>,
    pub resend: Option<ResendConfig>,
    pub smtp: Option<SmtpConfig>,
    pub mailpit: Option<MailpitConfig>,
}

#[derive(Debug, Clone)]
pub struct ResendConfig {
    pub api_key: String,
}

#[derive(Debug, Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MailpitConfig {
    pub host: String,
    pub port: u16,
}

/// `[config.storage]` — physical buckets + the logical→physical mapping.
#[derive(Debug, Clone, Default)]
pub struct StorageConfig {
    /// Physical bucket name → its connection + capability flags.
    pub buckets: BTreeMap<String, PhysicalBucket>,
    /// `"<plugin>:<logical>"` → physical bucket name.
    pub mapping: BTreeMap<String, String>,
    /// HMAC secret for signing host-mediated upload/download tokens (Stage 7).
    pub token_secret: Option<String>,
}

/// One physical object-storage bucket + the provider capabilities the host may
/// rely on (anything the provider lacks, juniusd serves itself — Stage 7).
#[derive(Debug, Clone)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent provider capability + visibility flags"
)]
pub struct PhysicalBucket {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    /// `MinIO` and some `S3`-compatibles need path-style addressing.
    pub use_path_style: bool,
    /// Objects in this bucket are world-readable (yields stable public URLs).
    pub public: bool,
    /// The provider supports presigned PUT (else juniusd mediates uploads).
    pub presigned_put: bool,
    /// The provider supports presigned GET (else juniusd mediates downloads).
    pub presigned_get: bool,
    /// The provider serves anonymous GET for `public` objects.
    pub public_get: bool,
}

// --- field helpers over a `toml` table --------------------------------------

fn sub<'a>(t: &'a Table, key: &str) -> Option<&'a Table> {
    t.get(key).and_then(toml::Value::as_table)
}

fn str_req(t: &Table, key: &str) -> Result<String, SecretError> {
    t.get(key)
        .and_then(toml::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| SecretError::MissingKey(key.to_string()))
}

fn secret_req(t: &Table, key: &str, lookup: Lookup) -> Result<String, SecretError> {
    SecretRef::from_str(&str_req(t, key)?)?.resolve(&lookup)
}

fn secret_opt(t: &Table, key: &str, lookup: Lookup) -> Result<Option<String>, SecretError> {
    match t.get(key).and_then(toml::Value::as_str) {
        None => Ok(None),
        Some(s) => Ok(Some(SecretRef::from_str(s)?.resolve(&lookup)?)),
    }
}

fn bool_or(t: &Table, key: &str, default: bool) -> bool {
    t.get(key).and_then(toml::Value::as_bool).unwrap_or(default)
}

fn u16_or(t: &Table, key: &str, default: u16) -> u16 {
    t.get(key)
        .and_then(toml::Value::as_integer)
        .and_then(|n| u16::try_from(n).ok())
        .unwrap_or(default)
}

fn i64_or(t: &Table, key: &str, default: i64) -> i64 {
    t.get(key)
        .and_then(toml::Value::as_integer)
        .unwrap_or(default)
}

fn string_vec(t: &Table, key: &str) -> Vec<String> {
    t.get(key)
        .and_then(toml::Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(toml::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

// --- section parsers ---------------------------------------------------------

pub(crate) fn parse_jobs(cfg: &Table, lookup: Lookup) -> Result<Option<JobsConfig>, SecretError> {
    let Some(t) = sub(cfg, "jobs") else {
        return Ok(None);
    };
    Ok(Some(JobsConfig {
        amqp_url: secret_req(t, "amqp_url", lookup)?,
    }))
}

pub(crate) fn parse_job_workers(cfg: &Table) -> u16 {
    u16_or(cfg, "job_workers", 4)
}

pub(crate) fn parse_otel(cfg: &Table) -> OtelConfig {
    let Some(t) = sub(cfg, "otel") else {
        return OtelConfig::default();
    };
    OtelConfig {
        enabled: bool_or(t, "enabled", false),
        endpoint: t
            .get("endpoint")
            .and_then(toml::Value::as_str)
            .map(str::to_string),
    }
}

pub(crate) fn parse_audit(cfg: &Table) -> AuditConfig {
    match sub(cfg, "audit") {
        Some(t) => AuditConfig {
            retention_days: i64_or(t, "retention_days", 365),
        },
        None => AuditConfig::default(),
    }
}

pub(crate) fn parse_email(cfg: &Table, lookup: Lookup) -> Result<Option<EmailConfig>, SecretError> {
    let Some(t) = sub(cfg, "email") else {
        return Ok(None);
    };
    let resend = match sub(t, "resend") {
        Some(r) => Some(ResendConfig {
            api_key: secret_req(r, "api_key", lookup)?,
        }),
        None => None,
    };
    let smtp = match sub(t, "smtp") {
        Some(s) => Some(SmtpConfig {
            host: str_req(s, "host")?,
            port: u16_or(s, "port", 587),
            username: secret_opt(s, "username", lookup)?,
            password: secret_opt(s, "password", lookup)?,
        }),
        None => None,
    };
    let mailpit = match sub(t, "mailpit") {
        Some(m) => Some(MailpitConfig {
            host: str_req(m, "host")?,
            port: u16_or(m, "port", 1025),
        }),
        None => None,
    };
    Ok(Some(EmailConfig {
        transport: t
            .get("transport")
            .and_then(toml::Value::as_str)
            .unwrap_or("log")
            .to_string(),
        from_default: str_req(t, "from_default")?,
        allowed_sender_domains: string_vec(t, "allowed_sender_domains"),
        resend,
        smtp,
        mailpit,
    }))
}

pub(crate) fn parse_storage(
    cfg: &Table,
    lookup: Lookup,
) -> Result<Option<StorageConfig>, SecretError> {
    let Some(t) = sub(cfg, "storage") else {
        return Ok(None);
    };
    let mut buckets = BTreeMap::new();
    if let Some(bucket_tables) = sub(t, "buckets") {
        for (name, value) in bucket_tables {
            let Some(b) = value.as_table() else {
                continue;
            };
            buckets.insert(
                name.clone(),
                PhysicalBucket {
                    endpoint: str_req(b, "endpoint")?,
                    region: str_req(b, "region")?,
                    bucket: str_req(b, "bucket")?,
                    access_key: secret_req(b, "access_key", lookup)?,
                    secret_key: secret_req(b, "secret_key", lookup)?,
                    use_path_style: bool_or(b, "use_path_style", false),
                    public: bool_or(b, "public", false),
                    presigned_put: bool_or(b, "presigned_put", true),
                    presigned_get: bool_or(b, "presigned_get", true),
                    public_get: bool_or(b, "public_get", false),
                },
            );
        }
    }
    let mut mapping = BTreeMap::new();
    if let Some(map_table) = sub(t, "mapping") {
        for (logical, value) in map_table {
            if let Some(physical) = value.as_str() {
                mapping.insert(logical.clone(), physical.to_string());
            }
        }
    }
    Ok(Some(StorageConfig {
        buckets,
        mapping,
        token_secret: secret_opt(t, "token_secret", lookup)?,
    }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests panic on unexpected failures")]
mod tests {
    use super::*;

    fn tbl(s: &str) -> Table {
        toml::from_str(s).unwrap()
    }

    fn no_env() -> impl Fn(&str) -> Option<String> {
        |_| None
    }

    #[test]
    fn absent_sections_are_none_or_default() {
        let cfg = tbl("database_url = \"x\"\n");
        assert!(parse_jobs(&cfg, &no_env()).unwrap().is_none());
        assert!(parse_email(&cfg, &no_env()).unwrap().is_none());
        assert!(parse_storage(&cfg, &no_env()).unwrap().is_none());
        assert_eq!(parse_job_workers(&cfg), 4);
        assert!(!parse_otel(&cfg).enabled);
        assert_eq!(parse_audit(&cfg).retention_days, 365);
    }

    #[test]
    fn jobs_resolves_env() {
        let cfg = tbl("[jobs]\namqp_url = \"env:AMQP\"\n");
        let env = |k: &str| (k == "AMQP").then(|| "amqp://h".to_string());
        assert_eq!(
            parse_jobs(&cfg, &env).unwrap().unwrap().amqp_url,
            "amqp://h"
        );
    }

    #[test]
    fn otel_audit_workers_overrides() {
        let cfg = tbl(
            "job_workers = 8\n[otel]\nenabled = true\nendpoint = \"http://x:4317\"\n\
             [audit]\nretention_days = 30\n",
        );
        assert_eq!(parse_job_workers(&cfg), 8);
        let o = parse_otel(&cfg);
        assert!(o.enabled);
        assert_eq!(o.endpoint.as_deref(), Some("http://x:4317"));
        assert_eq!(parse_audit(&cfg).retention_days, 30);
    }

    #[test]
    fn email_mailpit_section() {
        let cfg = tbl(
            "[email]\ntransport = \"mailpit\"\nfrom_default = \"no-reply@local\"\n\
             allowed_sender_domains = [\"local\"]\n[email.mailpit]\nhost = \"localhost\"\nport = 1025\n",
        );
        let e = parse_email(&cfg, &no_env()).unwrap().unwrap();
        assert_eq!(e.transport, "mailpit");
        assert_eq!(e.allowed_sender_domains, vec!["local".to_string()]);
        assert_eq!(e.mailpit.unwrap().port, 1025);
        assert!(e.smtp.is_none());
    }

    #[test]
    fn email_smtp_and_resend_resolve_secrets() {
        let cfg = tbl("[email]\ntransport = \"smtp\"\nfrom_default = \"a@b\"\n\
             [email.smtp]\nhost = \"s\"\nport = 587\nusername = \"env:U\"\npassword = \"env:P\"\n\
             [email.resend]\napi_key = \"env:RK\"\n");
        let env = |k: &str| match k {
            "U" => Some("user".to_string()),
            "P" => Some("pass".to_string()),
            "RK" => Some("rk".to_string()),
            _ => None,
        };
        let e = parse_email(&cfg, &env).unwrap().unwrap();
        let s = e.smtp.unwrap();
        assert_eq!(s.username.as_deref(), Some("user"));
        assert_eq!(s.password.as_deref(), Some("pass"));
        assert_eq!(e.resend.unwrap().api_key, "rk");
    }

    #[test]
    fn storage_buckets_mapping_and_flags() {
        let cfg = tbl("[storage]\ntoken_secret = \"env:TS\"\n\
             [storage.buckets.main]\nendpoint = \"http://localhost:9100\"\nregion = \"us-east-1\"\n\
             bucket = \"platform-dev\"\naccess_key = \"env:AK\"\nsecret_key = \"env:SK\"\n\
             use_path_style = true\npresigned_put = false\n\
             [storage.mapping]\n\"hello:attachments\" = \"main\"\n");
        let env = |k: &str| match k {
            "TS" => Some("ts".to_string()),
            "AK" => Some("ak".to_string()),
            "SK" => Some("sk".to_string()),
            _ => None,
        };
        let s = parse_storage(&cfg, &env).unwrap().unwrap();
        let b = &s.buckets["main"];
        assert_eq!(b.bucket, "platform-dev");
        assert_eq!(b.access_key, "ak");
        assert!(b.use_path_style);
        assert!(!b.presigned_put);
        assert!(b.presigned_get); // default true
        assert_eq!(s.mapping["hello:attachments"], "main");
        assert_eq!(s.token_secret.as_deref(), Some("ts"));
    }
}
