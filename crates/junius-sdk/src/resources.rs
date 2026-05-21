//! `PluginResources` — the bundle of host-provided handles passed to a plugin
//! at `routes()` time and during lifecycle hooks. M02 ships only `config` and
//! `telemetry`; later milestones add `db`, `storage`, `jobs`, `email`, `auth`,
//! `audit`.

use crate::config::PluginConfig;
use crate::telemetry::Telemetry;

#[derive(Clone)]
pub struct PluginResources {
    pub config: PluginConfig,
    pub telemetry: Telemetry,
}

impl PluginResources {
    pub fn new(config: PluginConfig, telemetry: Telemetry) -> Self {
        Self { config, telemetry }
    }
}
