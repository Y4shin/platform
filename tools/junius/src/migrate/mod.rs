//! The migration runner: discover host + plugin migrations, order them as a DAG,
//! apply pending ones transactionally against Postgres, then emit per-plugin
//! Postgres role grants. See [`docs/design/10-infrastructure-and-data.md`] §10.5.

pub mod checksum;
pub mod dag;
pub mod discover;
pub mod header;
pub mod runner;

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use junius_manifest::{
    PluginManifest, ResolvedConfig, compute_grants, derive_role_password, emit_grant_sql,
};

use discover::{HOST_PLUGIN, MigrationFile};
use header::{RequireEdge, parse_requires};

/// The bootstrap migration that creates `meta.migrations`; the runner applies it
/// first when the bookkeeping table is absent.
const BOOTSTRAP_MIGRATION: &str = "0006_meta_migrations";

#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("migration ordering: {0}")]
    Dag(String),
    #[error(
        "checksum mismatch for {plugin}:{name} — its .up.sql changed after it was applied; \
         migrations are immutable once applied (forward-fix with a new migration)"
    )]
    ChecksumMismatch { plugin: String, name: String },
    #[error(
        "bootstrap migration platform:{BOOTSTRAP_MIGRATION} not found under platform/migrations"
    )]
    MissingBootstrap,
}

struct LoadedMigration {
    file: MigrationFile,
    sql: String,
    checksum: String,
    requires: Vec<RequireEdge>,
}

/// Outcome of `migrate up`.
#[derive(Debug)]
pub struct UpReport {
    /// `plugin:name` labels of migrations applied this run.
    pub applied: Vec<String>,
    /// Count of migrations already applied (skipped).
    pub already_applied: usize,
    /// Role names whose grants were (re-)emitted.
    pub roles_granted: Vec<String>,
}

/// Outcome of `migrate status`.
pub struct StatusReport {
    pub applied: Vec<String>,
    pub pending: Vec<String>,
}

/// Apply all pending migrations, then emit per-plugin role grants.
pub async fn up(
    repo_root: &Path,
    config: &ResolvedConfig,
    manifests: &BTreeMap<String, PluginManifest>,
    enabled: &[String],
) -> Result<UpReport, MigrateError> {
    let loaded = load_all(repo_root, enabled)?;
    let ordered = order_loaded(&loaded)?;

    let pool = runner::connect(&config.database_url).await?;

    // Bootstrap meta.migrations (apply 0006 first) when the database is fresh.
    if !runner::meta_exists(&pool).await? {
        let boot = loaded
            .iter()
            .find(|m| m.file.plugin == HOST_PLUGIN && m.file.name == BOOTSTRAP_MIGRATION)
            .ok_or(MigrateError::MissingBootstrap)?;
        runner::apply(
            &pool,
            &boot.file.plugin,
            &boot.file.name,
            &boot.sql,
            &boot.checksum,
        )
        .await?;
    }

    let already = runner::applied(&pool).await?;
    let mut applied = Vec::new();
    let mut already_applied = 0usize;
    for &i in &ordered {
        let m = &loaded[i];
        let key = (m.file.plugin.clone(), m.file.name.clone());
        if let Some(existing) = already.get(&key) {
            if existing != &m.checksum {
                return Err(MigrateError::ChecksumMismatch {
                    plugin: m.file.plugin.clone(),
                    name: m.file.name.clone(),
                });
            }
            already_applied += 1;
            continue;
        }
        runner::apply(&pool, &m.file.plugin, &m.file.name, &m.sql, &m.checksum).await?;
        applied.push(format!("{}:{}", m.file.plugin, m.file.name));
    }

    // Emit per-plugin role grants (idempotent) once schemas exist.
    let mut roles_granted = Vec::new();
    for name in enabled {
        if let Some(manifest) = manifests.get(name) {
            let grant = compute_grants(manifest, manifests);
            let password = derive_role_password(&config.role_password_secret, &grant.role);
            runner::run_sql(&pool, &emit_grant_sql(&grant, &password)).await?;
            roles_granted.push(grant.role);
        }
    }

    Ok(UpReport {
        applied,
        already_applied,
        roles_granted,
    })
}

/// Report applied vs pending migrations without changing anything.
pub async fn status(
    repo_root: &Path,
    config: &ResolvedConfig,
    enabled: &[String],
) -> Result<StatusReport, MigrateError> {
    let loaded = load_all(repo_root, enabled)?;
    let ordered = order_loaded(&loaded)?;

    let pool = runner::connect(&config.database_url).await?;
    let already = if runner::meta_exists(&pool).await? {
        runner::applied(&pool).await?
    } else {
        HashMap::new()
    };

    let mut applied = Vec::new();
    let mut pending = Vec::new();
    for &i in &ordered {
        let m = &loaded[i];
        let label = format!("{}:{}", m.file.plugin, m.file.name);
        if already.contains_key(&(m.file.plugin.clone(), m.file.name.clone())) {
            applied.push(label);
        } else {
            pending.push(label);
        }
    }
    Ok(StatusReport { applied, pending })
}

fn load_all(repo_root: &Path, enabled: &[String]) -> Result<Vec<LoadedMigration>, MigrateError> {
    let files = discover::discover(repo_root, enabled)?;
    let mut loaded = Vec::with_capacity(files.len());
    for file in files {
        let sql = std::fs::read_to_string(&file.path)?;
        let checksum = checksum::sha256_hex(sql.as_bytes());
        let requires = parse_requires(&sql);
        loaded.push(LoadedMigration {
            file,
            sql,
            checksum,
            requires,
        });
    }
    Ok(loaded)
}

fn order_loaded(loaded: &[LoadedMigration]) -> Result<Vec<usize>, MigrateError> {
    let inputs: Vec<dag::OrderInput> = loaded
        .iter()
        .map(|m| dag::OrderInput {
            plugin: &m.file.plugin,
            name: &m.file.name,
            requires: &m.requires,
        })
        .collect();
    dag::order(&inputs).map_err(|e| {
        MigrateError::Dag(match e {
            dag::DagError::Cycle(cycle) => format!("dependency cycle: {}", cycle.join(" -> ")),
            dag::DagError::UnknownRequire { from, target } => {
                format!("{from} requires {target}, which does not exist")
            }
        })
    })
}
