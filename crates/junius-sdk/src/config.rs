//! Per-plugin typed config access. The host loads the deployment's
//! `[plugins.<name>]` table from `platform.toml` and hands it to each plugin
//! as a `PluginConfig`.

use std::sync::Arc;

use serde::de::DeserializeOwned;

use crate::error::PluginError;

#[derive(Clone, Default)]
pub struct PluginConfig {
    inner: Arc<toml::Table>,
}

impl PluginConfig {
    /// A `PluginConfig` with no values. Used by the host for plugins that the
    /// deployment didn't supply a `[plugins.<name>]` table for.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Wrap a parsed TOML table (the per-plugin override block).
    pub fn from_table(table: toml::Table) -> Self {
        Self {
            inner: Arc::new(table),
        }
    }

    /// Deserialise the value at `key` into `T`. Returns
    /// [`PluginError::Config`] if missing or if deserialisation fails.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<T, PluginError> {
        let value = self
            .inner
            .get(key)
            .ok_or_else(|| PluginError::Config(format!("missing config key {key:?}")))?;
        value
            .clone()
            .try_into::<T>()
            .map_err(|e| PluginError::Config(format!("deserialise {key:?}: {e}")))
    }

    /// Like [`Self::get`] but returns `Ok(None)` for a missing key.
    pub fn get_opt<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, PluginError> {
        match self.inner.get(key) {
            None => Ok(None),
            Some(v) => v
                .clone()
                .try_into::<T>()
                .map(Some)
                .map_err(|e| PluginError::Config(format!("deserialise {key:?}: {e}"))),
        }
    }
}
