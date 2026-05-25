//! Host telemetry wiring (M10 Stage 3). `tracing` is the only telemetry API the
//! host and plugins touch; OpenTelemetry is **export-only**. When
//! `[config.otel] enabled = true`, spans, log events, and metrics are exported
//! over OTLP/gRPC to the Grafana LGTM stack (`:4317`):
//!
//! - **traces** via [`tracing_opentelemetry`] (spans → `OTel` traces),
//! - **logs** via [`opentelemetry_appender_tracing`] (events → `OTel` logs),
//! - **metrics** via an [`OtelMetricSink`] bridged into each plugin's
//!   [`Telemetry`](junius_sdk::Telemetry) through the SDK's `MetricSink` trait.
//!
//! Every concrete `opentelemetry` crate lives here in the host — never in
//! `junius-sdk`, which speaks `tracing` + the vendor-neutral `MetricSink` trait.

use std::sync::Arc;

use anyhow::Context as _;
use junius_manifest::OtelConfig;
use junius_sdk::MetricSink;
use opentelemetry::metrics::{Meter, MeterProvider as _};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{KeyValue, global};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{LogExporter, MetricExporter, SpanExporter, WithExportConfig as _};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

/// `service.name` reported on every exported span/log/metric.
const SERVICE_NAME: &str = "platform";

/// Keeps the `OTel` export pipelines alive for the process lifetime and flushes
/// them on drop. On the disabled path every field is `None` and drop is a no-op.
/// The guard must outlive [`server::run`](crate::server::run) so in-flight
/// telemetry is drained on shutdown.
#[derive(Default)]
pub struct TelemetryGuard {
    tracer: Option<SdkTracerProvider>,
    logger: Option<SdkLoggerProvider>,
    meter: Option<SdkMeterProvider>,
}

impl TelemetryGuard {
    /// Whether any `OTel` export pipeline is wired (false on the disabled path).
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.tracer.is_some() || self.logger.is_some() || self.meter.is_some()
    }
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        // Best-effort flush: `shutdown` drains the batch/periodic processors. An
        // error here only means some final telemetry may be lost on exit.
        if let Some(p) = &self.tracer {
            let _ = p.shutdown();
        }
        if let Some(p) = &self.logger {
            let _ = p.shutdown();
        }
        if let Some(p) = &self.meter {
            let _ = p.shutdown();
        }
    }
}

/// Install the global `tracing` subscriber — always an `RUST_LOG`-filtered fmt
/// layer, plus (when `otel.enabled`) the OTLP export layers for traces and logs
/// and a metrics pipeline. Returns a guard that flushes the pipelines on drop
/// and the host [`MetricSink`] (present only when `OTel` is on) to wire into
/// plugin telemetry.
///
/// Must be called from within the Tokio runtime: the tonic OTLP client needs it.
pub fn init_telemetry(
    otel: &OtelConfig,
) -> anyhow::Result<(TelemetryGuard, Option<Arc<dyn MetricSink>>)> {
    // `RUST_LOG` is the standard developer-debug knob; `EnvFilter` reads it for
    // us (no direct env read here — see `platform::config` for the policy).
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let fmt_layer = tracing_subscriber::fmt::layer().with_target(false);

    if !otel.enabled {
        // Disabled: fmt layer only — no OTLP wiring, no metric sink.
        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .try_init();
        return Ok((TelemetryGuard::default(), None));
    }

    let resource = Resource::builder().with_service_name(SERVICE_NAME).build();

    // Traces: spans → OTel traces via the tracing-opentelemetry bridge.
    let tracer_provider = SdkTracerProvider::builder()
        .with_batch_exporter(build_span_exporter(otel)?)
        .with_resource(resource.clone())
        .build();
    let trace_layer =
        tracing_opentelemetry::layer().with_tracer(tracer_provider.tracer(SERVICE_NAME));

    // Logs: `tracing` events → OTel logs via the appender bridge.
    let logger_provider = SdkLoggerProvider::builder()
        .with_batch_exporter(build_log_exporter(otel)?)
        .with_resource(resource.clone())
        .build();
    let log_layer = OpenTelemetryTracingBridge::new(&logger_provider);

    // Metrics: a periodic-export meter the host bridges plugin metrics onto.
    let meter_provider = SdkMeterProvider::builder()
        .with_periodic_exporter(build_metric_exporter(otel)?)
        .with_resource(resource)
        .build();
    let sink: Arc<dyn MetricSink> = Arc::new(OtelMetricSink {
        meter: meter_provider.meter(SERVICE_NAME),
    });

    // Make the tracer/meter discoverable globally (context propagation, and any
    // `global::meter(...)` callers).
    global::set_tracer_provider(tracer_provider.clone());
    global::set_meter_provider(meter_provider.clone());

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .with(trace_layer)
        .with(log_layer)
        .try_init();

    Ok((
        TelemetryGuard {
            tracer: Some(tracer_provider),
            logger: Some(logger_provider),
            meter: Some(meter_provider),
        },
        Some(sink),
    ))
}

/// Apply the configured endpoint, if any, leaving the OTLP default (`:4317`)
/// otherwise.
fn build_span_exporter(otel: &OtelConfig) -> anyhow::Result<SpanExporter> {
    let mut builder = SpanExporter::builder().with_tonic();
    if let Some(endpoint) = &otel.endpoint {
        builder = builder.with_endpoint(endpoint.as_str());
    }
    builder.build().context("building OTLP span exporter")
}

fn build_log_exporter(otel: &OtelConfig) -> anyhow::Result<LogExporter> {
    let mut builder = LogExporter::builder().with_tonic();
    if let Some(endpoint) = &otel.endpoint {
        builder = builder.with_endpoint(endpoint.as_str());
    }
    builder.build().context("building OTLP log exporter")
}

fn build_metric_exporter(otel: &OtelConfig) -> anyhow::Result<MetricExporter> {
    let mut builder = MetricExporter::builder().with_tonic();
    if let Some(endpoint) = &otel.endpoint {
        builder = builder.with_endpoint(endpoint.as_str());
    }
    builder.build().context("building OTLP metric exporter")
}

/// Host [`MetricSink`] that records plugin metrics on the `OTel` meter. The SDK
/// deduplicates instruments by name, so building the counter/histogram per call
/// is cheap and returns the same underlying instrument.
struct OtelMetricSink {
    meter: Meter,
}

impl MetricSink for OtelMetricSink {
    fn counter(&self, name: &str, value: u64, attributes: &[(&str, &str)]) {
        self.meter
            .u64_counter(name.to_string())
            .build()
            .add(value, &to_key_values(attributes));
    }

    fn histogram(&self, name: &str, value: f64, attributes: &[(&str, &str)]) {
        self.meter
            .f64_histogram(name.to_string())
            .build()
            .record(value, &to_key_values(attributes));
    }
}

fn to_key_values(attributes: &[(&str, &str)]) -> Vec<KeyValue> {
    attributes
        .iter()
        .map(|(k, v)| KeyValue::new((*k).to_string(), (*v).to_string()))
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests panic on unexpected failures")]
mod tests {
    use super::*;

    #[test]
    fn disabled_otel_installs_no_pipelines_and_no_sink() {
        // The disabled path must not wire OTLP nor hand back a metric sink — it
        // is purely the fmt layer. (Safe to call even if a global subscriber is
        // already set in this test process: `try_init` is best-effort.)
        let (guard, sink) = init_telemetry(&OtelConfig::default()).unwrap();
        assert!(!guard.is_active(), "no OTel providers should be built");
        assert!(sink.is_none(), "no metric sink without OTel");
    }
}
