//! Host-global infra clients, built once at boot from the resolved deployment
//! config and threaded into each plugin's `PluginResourceCtx` via
//! [`boot::build_ctx`](crate::boot::build_ctx).
//!
//! The concrete backends (lapin / aws-sdk-s3 / lettre / OpenTelemetry) live here
//! in the host, **behind the SDK's vendor-neutral traits** — they never appear
//! in `junius-sdk`. Stage 3 added the metrics sink; Stage 4 adds email. Later
//! stages add the job backend and object stores.

use std::collections::HashMap;
use std::sync::Arc;

use junius_manifest::{ProvisioningConfig, ResolvedConfig};
use junius_sdk::{
    AuditEmitter, Email, JobBackend, Jobs, MetricSink, ObjectStore, PlatformAdminApi,
    PluginPermissionsSummary, PluginStorage, Transport, UrlSigner,
};
use sqlx::PgPool;

use crate::storage::http::HostUrlSigner;
use crate::storage::token::TokenSigner;

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
    /// Object stores (one per physical bucket) + the logical→physical mapping.
    pub storage: StorageInfra,
    /// Aggregated permission catalogue used by the M18 admin plugin: one
    /// entry per loaded plugin, each carrying its `[permissions]` block.
    /// Built once at host start from every plugin's `PluginMetadata`.
    pub admin_catalogue: Arc<Vec<PluginPermissionsSummary>>,
    /// M18 Stage D — when `true`, the `PlatformAdminApi` refuses mutations
    /// targeting rows with `managed_by='config'`. Defaults to `true` (the
    /// safe choice); a deployment can flip it off via
    /// `[provisioning] lock_managed = false`.
    pub admin_lock_managed: bool,
}

/// The deployment's object storage: one [`ObjectStore`] per physical bucket, the
/// `"<plugin>:<logical>" → physical` mapping, and (when a token secret is set)
/// the signer for juniusd-mediated URLs + the token signer the host endpoints use.
#[derive(Clone, Default)]
pub struct StorageInfra {
    pub stores: Arc<HashMap<String, Arc<dyn ObjectStore>>>,
    pub mapping: Arc<HashMap<String, String>>,
    pub signer: Option<Arc<dyn UrlSigner>>,
    pub token_signer: Option<TokenSigner>,
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
        provisioning: Option<&ProvisioningConfig>,
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
        let storage = match &resolved.storage {
            Some(cfg) => {
                let token_signer = cfg.token_secret.as_deref().map(TokenSigner::new);
                let signer: Option<Arc<dyn UrlSigner>> = token_signer
                    .clone()
                    .map(|ts| Arc::new(HostUrlSigner::new(ts)) as Arc<dyn UrlSigner>);
                StorageInfra {
                    stores: Arc::new(crate::storage::build_object_stores(cfg).await?),
                    mapping: Arc::new(cfg.mapping.clone().into_iter().collect()),
                    signer,
                    token_signer,
                }
            }
            None => StorageInfra::default(),
        };
        let admin_lock_managed = provisioning.is_none_or(|p| p.lock_managed);
        Ok(Self {
            metric_sink,
            email,
            jobs,
            storage,
            admin_catalogue: Arc::new(Vec::new()),
            admin_lock_managed,
        })
    }

    /// Attach the aggregated permission catalogue. Called once at server
    /// startup after the plugin registry is materialised; before this is set,
    /// `admin_handle` returns an API with an empty catalogue (still functional
    /// — only `permission_catalogue()` is affected).
    #[must_use]
    pub fn with_admin_catalogue(mut self, catalogue: Arc<Vec<PluginPermissionsSummary>>) -> Self {
        self.admin_catalogue = catalogue;
        self
    }

    /// Build the per-plugin [`PlatformAdminApi`] handle, gated on
    /// `platform.admin`. Every method on the returned API still checks the
    /// capability at entry; this just hands the plugin its handle.
    #[must_use]
    pub fn admin_handle(
        &self,
        platform_pool: &PgPool,
        plugin_name: &'static str,
        capabilities: &'static [&'static str],
    ) -> PlatformAdminApi {
        PlatformAdminApi::new(
            platform_pool.clone(),
            AuditEmitter::new(platform_pool.clone()),
            plugin_name,
            capabilities,
            self.admin_catalogue.clone(),
            self.admin_lock_managed,
        )
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

    /// Build the per-plugin [`PluginStorage`] handle: this plugin's logical→
    /// physical bucket map + the shared object stores, gated on `capabilities`.
    #[must_use]
    pub fn storage_handle(
        &self,
        platform_pool: &PgPool,
        plugin_name: &'static str,
        capabilities: &'static [&'static str],
    ) -> PluginStorage {
        if self.storage.stores.is_empty() {
            return PluginStorage::empty(plugin_name, capabilities);
        }
        let prefix = format!("{plugin_name}:");
        let mapping: HashMap<String, String> = self
            .storage
            .mapping
            .iter()
            .filter_map(|(k, v)| {
                k.strip_prefix(&prefix)
                    .map(|logical| (logical.to_string(), v.clone()))
            })
            .collect();
        PluginStorage::new(
            (*self.storage.stores).clone(),
            mapping,
            plugin_name,
            capabilities,
            Some(platform_pool.clone()),
            self.storage.signer.clone(),
        )
    }
}
