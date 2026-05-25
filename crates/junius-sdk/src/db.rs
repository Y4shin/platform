//! `PluginDb` — a plugin's opaque handle to its own Postgres connection pool.
//!
//! The pool authenticates as the plugin's least-privilege role (`role_<name>`),
//! so the database itself bounds what queries can touch. M06 intentionally
//! exposes **no** public way to run a query: the typed query API arrives in M07
//! with `#[derive(Repository)]`. Until then `PluginDb` is just a carrier.

use sqlx::PgPool;

/// A plugin's database handle. Holds the per-plugin connection pool but exposes
/// no executor publicly — query access is granted by the M07 repository derive.
#[derive(Clone)]
pub struct PluginDb {
    pool: PgPool,
    plugin_name: &'static str,
}

impl PluginDb {
    /// Construct the handle. Called by the host while building plugin pools.
    #[must_use]
    pub fn new(pool: PgPool, plugin_name: &'static str) -> Self {
        Self { pool, plugin_name }
    }

    /// The plugin this handle belongs to.
    #[must_use]
    pub fn plugin_name(&self) -> &'static str {
        self.plugin_name
    }

    /// The underlying pool. Crate-private: the repository layer (`ScopedDb`,
    /// built in this crate) reaches it here; plugins never get a raw executor.
    pub(crate) fn pool(&self) -> &PgPool {
        &self.pool
    }
}
