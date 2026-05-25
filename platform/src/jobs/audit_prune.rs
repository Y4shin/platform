//! Host audit-retention task (M10 Stage 8). NOT a plugin job — a host-scheduled
//! interval task that prunes `platform.audit_event` and `meta.job_run` rows
//! older than `[config.audit].retention_days`. Spawned in `server::run` and
//! cancelled with the job-worker token on shutdown.

use std::time::Duration;

use sqlx::PgPool;
use tokio_util::sync::CancellationToken;

/// How often the prune runs.
const INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// Delete audit + job-run rows older than `retention_days`. Returns the total
/// number of rows removed (for tests/observability).
pub async fn prune(pool: &PgPool, retention_days: i64) -> sqlx::Result<u64> {
    let days = i32::try_from(retention_days).unwrap_or(i32::MAX);
    let audit = sqlx::query(
        "DELETE FROM platform.audit_event WHERE occurred_at < now() - make_interval(days => $1)",
    )
    .bind(days)
    .execute(pool)
    .await?
    .rows_affected();
    let jobs = sqlx::query(
        "DELETE FROM meta.job_run WHERE enqueued_at < now() - make_interval(days => $1)",
    )
    .bind(days)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(audit + jobs)
}

/// Spawn the recurring prune. Runs once immediately, then every [`INTERVAL`]
/// until `cancel` fires.
pub fn spawn(
    pool: PgPool,
    retention_days: i64,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match prune(&pool, retention_days).await {
                Ok(n) if n > 0 => tracing::info!(rows = n, "audit prune removed expired rows"),
                Ok(_) => {}
                Err(e) => tracing::error!(error = %e, "audit prune failed"),
            }
            tokio::select! {
                () = cancel.cancelled() => break,
                () = tokio::time::sleep(INTERVAL) => {}
            }
        }
    })
}
