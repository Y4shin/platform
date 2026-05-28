//! Integration test for the `RabbitMQ`-backed job queue against ephemeral
//! Postgres + `RabbitMQ` containers: enqueue → worker consumes → handler runs with
//! the plugin's own resources → `job_run = completed`; an always-failing job
//! retries then dead-letters → `job_run = failed`. Graceful shutdown drains the
//! worker. Skips cleanly without Docker.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]

use std::time::Duration;

use junius_sdk::{
    AuditEmitter, Auth, Authz, Email, Groups, Job, JobHandler, Jobs, PluginConfig, PluginDb,
    PluginError, PluginResourceCtx, PluginStorage, SecretStore, Telemetry, Users,
};
use platform::jobs::worker::{JobRegistry, declare_job_queues, run_worker};
use platform::jobs::{build, declare_exchanges};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::rabbitmq::RabbitMq;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio_util::sync::CancellationToken;

const META_MIGRATION: &str = include_str!("../migrations/0006_meta_migrations.up.sql");
const JOB_RUN_MIGRATION: &str = include_str!("../migrations/0009_job_run.up.sql");

#[derive(Serialize, Deserialize)]
struct OkJob {
    msg: String,
}
impl Job for OkJob {
    const NAME: &'static str = "testjobs.ok";
}

#[derive(Serialize, Deserialize)]
struct FailJob {}
impl Job for FailJob {
    const NAME: &'static str = "testjobs.fail";
}

async fn job_status(pool: &PgPool, id: uuid::Uuid) -> Option<String> {
    sqlx::query_scalar("SELECT status FROM meta.job_run WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .unwrap()
}

async fn poll_status(pool: &PgPool, id: uuid::Uuid, want: &str) -> bool {
    for _ in 0..100 {
        if job_status(pool, id).await.as_deref() == Some(want) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

#[tokio::test]
async fn jobs_round_trip_and_dead_letter() {
    let pg = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping jobs_rabbitmq: Docker unavailable ({e})");
            return;
        }
    };
    let mq = match RabbitMq::default().start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping jobs_rabbitmq: RabbitMQ unavailable ({e})");
            return;
        }
    };
    let pg_port = pg.get_host_port_ipv4(5432).await.unwrap();
    let mq_port = mq.get_host_port_ipv4(5672).await.unwrap();
    let pool = PgPool::connect(&format!(
        "postgres://postgres:postgres@127.0.0.1:{pg_port}/postgres"
    ))
    .await
    .unwrap();
    sqlx::raw_sql(META_MIGRATION).execute(&pool).await.unwrap();
    sqlx::raw_sql(JOB_RUN_MIGRATION)
        .execute(&pool)
        .await
        .unwrap();

    let amqp_url = format!("amqp://guest:guest@127.0.0.1:{mq_port}/%2f");
    let (amqp_pool, backend) = build(&amqp_url).await.unwrap();

    // A channel the OK handler reports through, so we can assert it ran with the
    // right per-plugin resources and the deserialized payload.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let ok_handler = JobHandler::new::<OkJob, _, _>(move |job, resources| {
        let tx = tx.clone();
        async move {
            tx.send(format!("{}:{}", resources.telemetry.plugin_name(), job.msg))
                .unwrap();
            Ok(())
        }
    });
    let fail_handler = JobHandler::new::<FailJob, _, _>(|_job, _resources| async {
        Err(PluginError::External(anyhow::anyhow!("always fails")))
    });

    let ctx = PluginResourceCtx {
        config: PluginConfig::empty(),
        telemetry: Telemetry::new("testjobs"),
        db: PluginDb::new(pool.clone(), "testjobs"),
        auth: Auth::new(pool.clone()),
        users: Users::new(pool.clone()),
        groups: Groups::new(pool.clone()),
        audit: AuditEmitter::new(pool.clone()),
        authz: Authz::new(pool.clone()),
        email: Email::disabled("testjobs", &[]),
        jobs: Jobs::disabled("testjobs", &[]),
        storage: PluginStorage::empty("testjobs", &[]),
        platform_admin: junius_sdk::PlatformAdminApi::new(
            pool.clone(),
            AuditEmitter::new(pool.clone()),
            "testjobs",
            &[],
            std::sync::Arc::new(Vec::new()),
            false,
        ),
        localizer: junius_sdk::LocalizerBuilder::new(junius_sdk::Locale::En).build(),
        secrets: SecretStore::default(),
        capabilities: &["job.enqueue"],
    };
    let mut registry = JobRegistry::new();
    registry.register_plugin(vec![ok_handler, fail_handler], &ctx);

    // Pre-declare topology so messages enqueued before the worker's own declare
    // are routed to a bound queue (idempotent with the worker's declares).
    {
        let conn = amqp_pool.get().await.unwrap();
        let channel = conn.create_channel().await.unwrap();
        declare_exchanges(&channel).await.unwrap();
        declare_job_queues(&channel, OkJob::NAME).await.unwrap();
        declare_job_queues(&channel, FailJob::NAME).await.unwrap();
    }

    let cancel = CancellationToken::new();
    let worker = tokio::spawn(run_worker(
        amqp_pool.clone(),
        registry,
        pool.clone(),
        4,
        cancel.clone(),
    ));

    // Enqueue through the SDK handle (records job_run + publishes).
    let enqueue = Jobs::new(
        Some(backend.clone()),
        Some(pool.clone()),
        "testjobs",
        &["job.enqueue"],
    );
    let ok_id = enqueue
        .enqueue(OkJob {
            msg: "hello".to_string(),
        })
        .await
        .unwrap();
    let fail_id = enqueue.enqueue(FailJob {}).await.unwrap();

    // OK job completes and the handler reported the right plugin + payload.
    assert!(
        poll_status(&pool, ok_id, "completed").await,
        "ok job completed"
    );
    let reported = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reported, "testjobs:hello");

    // Failing job retries then dead-letters.
    assert!(
        poll_status(&pool, fail_id, "failed").await,
        "fail job failed"
    );
    let attempt: i32 = sqlx::query_scalar("SELECT attempt FROM meta.job_run WHERE id = $1")
        .bind(fail_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(attempt, 3, "should have exhausted all attempts");

    // Graceful shutdown drains the worker.
    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(10), worker)
        .await
        .expect("worker drained")
        .unwrap()
        .unwrap();
}
