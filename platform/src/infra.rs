//! Host-global infra clients, built once at boot from the resolved deployment
//! config and threaded into each plugin's `PluginResourceCtx` via
//! [`boot::build_ctx`](crate::boot::build_ctx).
//!
//! The concrete backends (lapin / aws-sdk-s3 / lettre / OpenTelemetry) live here
//! in the host, **behind the SDK's vendor-neutral traits** — they never appear
//! in `junius-sdk`. Later M10 stages add the job backend, object stores, and
//! email transport members; Stage 1 is the skeleton + the metrics sink hook.

use std::sync::Arc;

use junius_manifest::ResolvedConfig;
use junius_sdk::MetricSink;

/// Shared, request-independent infra handles. Cheap to clone (members are
/// `Arc`/`Clone`).
#[derive(Clone, Default)]
pub struct HostInfra {
    /// Bridges plugin `Telemetry` metrics to the host's `OTel` meter. `None` until
    /// `OTel` is wired (Stage 3) or when telemetry is disabled.
    pub metric_sink: Option<Arc<dyn MetricSink>>,
}

impl HostInfra {
    /// Build the host infra from the resolved deployment config. The `OTel` metric
    /// sink is constructed in [`telemetry::init_telemetry`](crate::telemetry::init_telemetry)
    /// (it needs the meter) and threaded in here. Later stages populate the
    /// job/storage/email members.
    #[allow(
        clippy::unused_async,
        reason = "later stages await backend client construction"
    )]
    pub async fn build(
        _resolved: &ResolvedConfig,
        metric_sink: Option<Arc<dyn MetricSink>>,
    ) -> anyhow::Result<Self> {
        Ok(Self { metric_sink })
    }
}
