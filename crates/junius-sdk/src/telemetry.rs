//! Per-plugin telemetry handle. Spans + structured logs go through `tracing`
//! (vendor-neutral); the host bridges them to OpenTelemetry/LGTM in M10. Metrics
//! go through the [`MetricSink`] trait so the SDK stays free of any
//! vendor/`OTel` crate — the host supplies the concrete sink (M10 Stage 3).

use std::sync::Arc;

/// Vendor-neutral metrics sink. The host implements this over its `OTel` meter;
/// the SDK never depends on `opentelemetry`. Every emission is tagged with the
/// plugin name by [`Telemetry`] before it reaches the sink.
pub trait MetricSink: Send + Sync {
    /// Add `value` to a monotonic counter named `name`.
    fn counter(&self, name: &str, value: u64, attributes: &[(&str, &str)]);
    /// Record `value` into a histogram named `name`.
    fn histogram(&self, name: &str, value: f64, attributes: &[(&str, &str)]);
}

/// Per-plugin telemetry handle: pre-tagged spans/logs (`tracing`) + metrics
/// (through an optional host [`MetricSink`]). Cheap to clone.
#[derive(Clone)]
pub struct Telemetry {
    plugin_name: &'static str,
    sink: Option<Arc<dyn MetricSink>>,
}

impl std::fmt::Debug for Telemetry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Telemetry")
            .field("plugin_name", &self.plugin_name)
            .field("sink", &self.sink.is_some())
            .finish()
    }
}

impl Telemetry {
    /// A telemetry handle with no metrics sink (spans/logs only). Used in tests
    /// and when `OTel` is disabled.
    #[must_use]
    pub fn new(plugin_name: &'static str) -> Self {
        Self {
            plugin_name,
            sink: None,
        }
    }

    /// A telemetry handle wired to the host's metrics sink (when configured).
    #[must_use]
    pub fn with_sink(plugin_name: &'static str, sink: Option<Arc<dyn MetricSink>>) -> Self {
        Self { plugin_name, sink }
    }

    #[must_use]
    pub fn plugin_name(&self) -> &'static str {
        self.plugin_name
    }

    /// Create a `tracing` info-level span tagged with `plugin = <name>` and
    /// `op = <name>`, following tokio's normal entered/dropped lifecycle.
    pub fn span(&self, op: &'static str) -> tracing::Span {
        tracing::info_span!("junius.plugin", plugin = %self.plugin_name, op = %op)
    }

    /// Add `value` to a counter (no-op when no sink is configured). Tagged with
    /// the plugin name.
    pub fn counter(&self, name: &str, value: u64) {
        if let Some(sink) = &self.sink {
            sink.counter(name, value, &[("plugin", self.plugin_name)]);
        }
    }

    /// Record `value` into a histogram (no-op when no sink is configured).
    pub fn histogram(&self, name: &str, value: f64) {
        if let Some(sink) = &self.sink {
            sink.histogram(name, value, &[("plugin", self.plugin_name)]);
        }
    }

    /// Emit a structured info log tagged with the plugin name. `fields` are
    /// rendered into the event (the `OTel` log appender exports it in M10).
    pub fn log_info(&self, msg: &str, fields: &[(&str, &str)]) {
        tracing::info!(plugin = %self.plugin_name, ?fields, "{msg}");
    }
}
