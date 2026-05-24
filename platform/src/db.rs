//! Host database bootstrap.
//!
//! [`DbBootstrap`] holds the privileged pool connected via the deployment's
//! `database_url` (the `platform_migrator` role); the host reuses it for
//! identity queries in v1. [`DbBootstrap::build_plugin_pools`] opens one pool
//! per plugin that authenticates as that plugin's least-privilege role
//! (`role_<name>`) using the password re-derived from `role_password_secret` —
//! the same derivation `junius migrate` used to create the role.

use std::collections::HashMap;
use std::str::FromStr;

use junius_manifest::{derive_role_password, role_name};
use junius_sdk::{Plugin, PluginDb};
use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

/// Host-owned database pools.
pub struct DbBootstrap {
    migrator_pool: PgPool,
}

impl DbBootstrap {
    /// Connect the privileged pool from the deployment `database_url`.
    pub async fn connect(database_url: &str) -> Result<Self, sqlx::Error> {
        let migrator_pool = PgPoolOptions::new()
            .max_connections(8)
            .connect(database_url)
            .await?;
        Ok(Self { migrator_pool })
    }

    /// The platform-scoped pool used for host identity/session/audit queries.
    #[must_use]
    pub fn platform_pool(&self) -> &PgPool {
        &self.migrator_pool
    }

    /// Build one pool per plugin, each authenticated as `role_<plugin>`.
    pub async fn build_plugin_pools(
        &self,
        database_url: &str,
        role_password_secret: &str,
        plugins: &[Box<dyn Plugin>],
    ) -> Result<PluginPools, sqlx::Error> {
        let base = PgConnectOptions::from_str(database_url)?;
        let mut pools = HashMap::new();
        for plugin in plugins {
            let name = plugin.metadata().name;
            let role = role_name(name);
            let password = derive_role_password(role_password_secret, &role);
            let opts = base.clone().username(&role).password(&password);
            let pool = PgPoolOptions::new()
                .max_connections(4)
                .connect_with(opts)
                .await?;
            pools.insert(name.to_string(), PluginDb::new(pool, name));
        }
        Ok(PluginPools { pools })
    }
}

/// Per-plugin database handles, keyed by plugin name.
pub struct PluginPools {
    pools: HashMap<String, PluginDb>,
}

impl PluginPools {
    /// An empty pool set (no plugins). Useful for tests that exercise only the
    /// host's own routes.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            pools: HashMap::new(),
        }
    }

    /// The handle for plugin `name`, if a pool was built for it.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<PluginDb> {
        self.pools.get(name).cloned()
    }
}
