//! Host-global infra clients, built once at boot from the resolved deployment
//! config and threaded into each plugin's `PluginResourceCtx` via
//! [`boot::build_ctx`](crate::boot::build_ctx).
//!
//! The concrete backends (lapin / aws-sdk-s3 / lettre / OpenTelemetry) live here
//! in the host, **behind the SDK's vendor-neutral traits** — they never appear
//! in `junius-sdk`. Stage 3 added the metrics sink; Stage 4 adds email. Later
//! stages add the job backend and object stores.

use std::sync::Arc;

use junius_manifest::ResolvedConfig;
use junius_sdk::{Email, JobBackend, Jobs, MetricSink, Transport};
use sqlx::PgPool;

/// Shared, request-independent infra handles. Cheap to clone (members are
/// `Arc`/`Clone`).
#[derive(Clone, Default)]
pub struct HostInfra {
    /// Bridges plugin `Telemetry` metrics to the host's `OTel` meter. `None` when
    /// telemetry is disabled.
    pub metric_sink: Option<Arc<dyn MetricSink>>,
    /// Outbound email transport + sender policy (`None` transport when no
    /// `[config.email]`).
    pub email: EmailInfra,
    /// Job broker backend + connection pool (`None` when no `[config.jobs]`).
    pub jobs: JobsInfra,
}

/// The deployment's job broker: the publish backend (for `enqueue`) + the
/// connection pool the worker consumes on.
#[derive(Clone, Default)]
pub struct JobsInfra {
    pub backend: Option<Arc<dyn JobBackend>>,
    pub pool: Option<deadpool_lapin::Pool>,
}

/// The deployment's email transport + sender policy, shared across plugins.
#[derive(Clone, Default)]
pub struct EmailInfra {
    /// The concrete transport, or `None` when email is unconfigured.
    pub transport: Option<Arc<dyn Transport>>,
    /// Default `from` address used when a message omits one.
    pub from_default: Arc<str>,
    /// Domains a plugin-supplied `from` is allowed to use.
    pub allowed_domains: Arc<[String]>,
}

impl HostInfra {
    /// Build the host infra from the resolved deployment config. The `OTel`
    /// metric sink is constructed in
    /// [`telemetry::init_telemetry`](crate::telemetry::init_telemetry) (it needs
    /// the meter) and threaded in here. Later stages populate the storage
    /// members.
    pub async fn build(
        resolved: &ResolvedConfig,
        metric_sink: Option<Arc<dyn MetricSink>>,
    ) -> anyhow::Result<Self> {
        let email = match &resolved.email {
            Some(cfg) => EmailInfra {
                transport: Some(crate::email::build_transport(cfg)?),
                from_default: Arc::from(cfg.from_default.as_str()),
                allowed_domains: Arc::from(cfg.allowed_sender_domains.clone()),
            },
            None => EmailInfra::default(),
        };
        let jobs = match &resolved.jobs {
            Some(cfg) => {
                let (pool, backend) = crate::jobs::build(&cfg.amqp_url).await?;
                JobsInfra {
                    backend: Some(backend),
                    pool: Some(pool),
                }
            }
            None => JobsInfra::default(),
        };
        Ok(Self {
            metric_sink,
            email,
            jobs,
        })
    }

    /// Build the per-plugin [`Email`] handle, gated on `capabilities`.
    #[must_use]
    pub fn email_handle(
        &self,
        plugin_name: &'static str,
        capabilities: &'static [&'static str],
    ) -> Email {
        Email::new(
            self.email.transport.clone(),
            self.email.from_default.clone(),
            self.email.allowed_domains.clone(),
            plugin_name,
            capabilities,
        )
    }

    /// Build the per-plugin [`Jobs`] handle, gated on `capabilities`. Uses the
    /// host `platform_pool` for the `meta.job_run` bookkeeping rows.
    #[must_use]
    pub fn jobs_handle(
        &self,
        platform_pool: &PgPool,
        plugin_name: &'static str,
        capabilities: &'static [&'static str],
    ) -> Jobs {
        Jobs::new(
            self.jobs.backend.clone(),
            self.jobs.backend.as_ref().map(|_| platform_pool.clone()),
            plugin_name,
            capabilities,
        )
    }
}
