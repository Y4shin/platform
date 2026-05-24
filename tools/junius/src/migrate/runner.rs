//! The database side of the migration runner: connect as `platform_migrator`,
//! bootstrap/inspect `meta.migrations`, and apply migrations transactionally.

use std::collections::HashMap;

use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};

/// Connect to Postgres using `database_url` (which carries the
/// `platform_migrator` role and password).
pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(4)
        .connect(database_url)
        .await
}

/// Whether `meta.migrations` exists yet (false on a fresh database).
pub async fn meta_exists(pool: &PgPool) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables \
         WHERE table_schema = 'meta' AND table_name = 'migrations')",
    )
    .fetch_one(pool)
    .await
}

/// The set of already-applied migrations: `(plugin, name) -> checksum`.
pub async fn applied(pool: &PgPool) -> Result<HashMap<(String, String), String>, sqlx::Error> {
    let rows = sqlx::query("SELECT plugin, migration_name, checksum FROM meta.migrations")
        .fetch_all(pool)
        .await?;
    let mut map = HashMap::with_capacity(rows.len());
    for row in rows {
        let plugin: String = row.try_get("plugin")?;
        let name: String = row.try_get("migration_name")?;
        let checksum: String = row.try_get("checksum")?;
        map.insert((plugin, name), checksum);
    }
    Ok(map)
}

/// Apply one migration and record it, in a single transaction. The migration
/// SQL may contain multiple statements (executed via the simple query protocol).
pub async fn apply(
    pool: &PgPool,
    plugin: &str,
    name: &str,
    sql: &str,
    checksum: &str,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::raw_sql(sql).execute(&mut *tx).await?;
    sqlx::query(
        "INSERT INTO meta.migrations (plugin, migration_name, checksum) VALUES ($1, $2, $3)",
    )
    .bind(plugin)
    .bind(name)
    .bind(checksum)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Run a standalone (idempotent) SQL block outside the migration set — used for
/// emitting role grants after migrations are applied.
pub async fn run_sql(pool: &PgPool, sql: &str) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(sql).execute(pool).await?;
    Ok(())
}
