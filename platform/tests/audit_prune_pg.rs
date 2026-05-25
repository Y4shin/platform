//! Integration test for the host audit-retention prune against ephemeral
//! Postgres: rows older than the retention window are deleted from
//! `platform.audit_event` + `meta.job_run`; recent rows are kept. Skips without
//! Docker.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use platform::jobs::audit_prune::prune;
use sqlx::PgPool;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_users.up.sql"),
    include_str!("../migrations/0006_meta_migrations.up.sql"),
    include_str!("../migrations/0007_audit_event.up.sql"),
    include_str!("../migrations/0009_job_run.up.sql"),
];

#[tokio::test]
async fn prune_deletes_expired_rows_only() {
    let pg = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping audit_prune_pg: Docker unavailable ({e})");
            return;
        }
    };
    let port = pg.get_host_port_ipv4(5432).await.unwrap();
    let pool = PgPool::connect(&format!(
        "postgres://postgres:postgres@127.0.0.1:{port}/postgres"
    ))
    .await
    .unwrap();
    for sql in MIGRATIONS {
        sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    }

    // One expired + one recent row in each table (retention = 365 days).
    sqlx::raw_sql(
        "INSERT INTO platform.audit_event (event_kind, occurred_at) \
           VALUES ('x:y.z', now() - interval '400 days'), ('x:y.z', now());
         INSERT INTO meta.job_run (job_name, plugin, payload, enqueued_at) \
           VALUES ('p.j', 'p', '{}'::jsonb, now() - interval '400 days'),
                  ('p.j', 'p', '{}'::jsonb, now());",
    )
    .execute(&pool)
    .await
    .unwrap();

    let removed = prune(&pool, 365).await.unwrap();
    assert_eq!(removed, 2, "one expired row from each table");

    let audit: i64 = sqlx::query_scalar("SELECT count(*) FROM platform.audit_event")
        .fetch_one(&pool)
        .await
        .unwrap();
    let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM meta.job_run")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(audit, 1, "recent audit row kept");
    assert_eq!(jobs, 1, "recent job_run kept");
}
