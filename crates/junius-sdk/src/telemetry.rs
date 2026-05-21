//! Per-plugin telemetry handle. At M02 this is a thin wrapper around `tracing`
//! that pre-tags spans with the plugin's name. The OpenTelemetry exporter and
//! counter/histogram helpers arrive in M10.

#[derive(Clone, Debug)]
pub struct Telemetry {
    plugin_name: &'static str,
}

impl Telemetry {
    pub fn new(plugin_name: &'static str) -> Self {
        Self { plugin_name }
    }

    pub fn plugin_name(&self) -> &'static str {
        self.plugin_name
    }

    /// Create a `tracing` info-level span tagged with `plugin = <name>` and
    /// `op = <name>`. Plugin code calls this to scope spans across an
    /// operation; the span follows tokio's normal entered/dropped lifecycle.
    pub fn span(&self, op: &'static str) -> tracing::Span {
        tracing::info_span!("junius.plugin", plugin = %self.plugin_name, op = %op)
    }
}
