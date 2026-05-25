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

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests panic on unexpected failures")]
mod tests {
    use std::sync::Mutex;

    use tracing::field::{Field, Visit};
    use tracing_subscriber::Layer;
    use tracing_subscriber::layer::{Context, SubscriberExt as _};
    use tracing_subscriber::registry::LookupSpan;

    use super::*;

    /// A recorded `(name, value, attributes)` metric call.
    type Call<V> = (String, V, Vec<(String, String)>);

    /// Records every metric call, with attributes owned for assertion.
    #[derive(Default)]
    struct RecordingSink {
        counters: Mutex<Vec<Call<u64>>>,
        histograms: Mutex<Vec<Call<f64>>>,
    }

    fn owned(attributes: &[(&str, &str)]) -> Vec<(String, String)> {
        attributes
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    impl MetricSink for RecordingSink {
        fn counter(&self, name: &str, value: u64, attributes: &[(&str, &str)]) {
            self.counters
                .lock()
                .unwrap()
                .push((name.to_string(), value, owned(attributes)));
        }
        fn histogram(&self, name: &str, value: f64, attributes: &[(&str, &str)]) {
            self.histograms
                .lock()
                .unwrap()
                .push((name.to_string(), value, owned(attributes)));
        }
    }

    #[test]
    fn metrics_forward_to_sink_tagged_with_plugin() {
        let sink = Arc::new(RecordingSink::default());
        let tel = Telemetry::with_sink("hello", Some(sink.clone()));
        tel.counter("requests", 3);
        tel.histogram("latency_ms", 12.5);

        let plugin_attr = vec![("plugin".to_string(), "hello".to_string())];
        let counters = sink.counters.lock().unwrap();
        assert_eq!(
            *counters,
            vec![("requests".to_string(), 3, plugin_attr.clone())]
        );
        let histograms = sink.histograms.lock().unwrap();
        assert_eq!(histograms.len(), 1);
        assert_eq!(histograms[0].0, "latency_ms");
        assert!((histograms[0].1 - 12.5).abs() < f64::EPSILON);
        assert_eq!(histograms[0].2, plugin_attr);
    }

    #[test]
    fn metrics_without_sink_are_noops() {
        // No sink configured (e.g. OTel disabled) — must not panic.
        let tel = Telemetry::new("hello");
        tel.counter("requests", 1);
        tel.histogram("latency_ms", 1.0);
    }

    /// Collects field names off any span/event via the default `record_debug`
    /// fan-in (all typed `record_*` forward to it).
    struct FieldKeys(Vec<String>);
    impl Visit for FieldKeys {
        fn record_debug(&mut self, field: &Field, _value: &dyn std::fmt::Debug) {
            self.0.push(field.name().to_string());
        }
    }

    /// Test subscriber layer capturing span names + span/event field keys.
    #[derive(Clone, Default)]
    struct Capture {
        span_names: Arc<Mutex<Vec<String>>>,
        span_fields: Arc<Mutex<Vec<String>>>,
        event_fields: Arc<Mutex<Vec<String>>>,
    }

    impl<S: tracing::Subscriber + for<'a> LookupSpan<'a>> Layer<S> for Capture {
        fn on_new_span(
            &self,
            attrs: &tracing::span::Attributes<'_>,
            _id: &tracing::span::Id,
            _ctx: Context<'_, S>,
        ) {
            self.span_names
                .lock()
                .unwrap()
                .push(attrs.metadata().name().to_string());
            let mut keys = FieldKeys(Vec::new());
            attrs.record(&mut keys);
            self.span_fields.lock().unwrap().extend(keys.0);
        }
        fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
            let mut keys = FieldKeys(Vec::new());
            event.record(&mut keys);
            self.event_fields.lock().unwrap().extend(keys.0);
        }
    }

    #[test]
    fn span_and_log_are_tagged_with_plugin() {
        let capture = Capture::default();
        let subscriber = tracing_subscriber::registry().with(capture.clone());
        tracing::subscriber::with_default(subscriber, || {
            let tel = Telemetry::new("hello");
            let span = tel.span("create");
            let _entered = span.enter();
            tel.log_info("created", &[("id", "42")]);
        });

        assert!(
            capture
                .span_names
                .lock()
                .unwrap()
                .iter()
                .any(|n| n == "junius.plugin"),
        );
        assert!(
            capture
                .span_fields
                .lock()
                .unwrap()
                .iter()
                .any(|k| k == "plugin")
        );
        assert!(
            capture
                .event_fields
                .lock()
                .unwrap()
                .iter()
                .any(|k| k == "plugin"),
        );
    }
}
