//! The in-process job worker. For each registered job it declares a durable
//! queue (+ a dead-letter queue) and consumes deliveries, dispatching each to
//! the plugin's handler with that plugin's own (caller-less) `PluginResources`.
//! Failures retry by re-publishing with an incremented attempt up to
//! [`MAX_ATTEMPTS`], after which the job is routed to its DLQ and recorded
//! `failed`. The worker drains in-flight jobs on cancellation.

use futures_lite::StreamExt as _;
use junius_sdk::{JobEnvelope, JobHandler, PluginResourceCtx, PluginResources};
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicPublishOptions, BasicQosOptions, QueueBindOptions,
    QueueDeclareOptions,
};
use lapin::types::{AMQPValue, FieldTable};
use lapin::{BasicProperties, Channel};
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::jobs::{JOBS_DLX, JOBS_EXCHANGE, declare_exchanges};

/// Total delivery attempts before a job is dead-lettered.
const MAX_ATTEMPTS: i32 = 3;

/// One registered job: its handler and the owning plugin's resource context.
struct Entry {
    handler: JobHandler,
    ctx: PluginResourceCtx,
}

/// Routing-key (job name) → handler + plugin context.
#[derive(Default)]
pub struct JobRegistry {
    entries: Vec<(String, Entry)>,
}

impl JobRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register every handler a plugin declared, all sharing that plugin's `ctx`.
    pub fn register_plugin(&mut self, handlers: Vec<JobHandler>, ctx: &PluginResourceCtx) {
        for handler in handlers {
            self.entries.push((
                handler.name().to_string(),
                Entry {
                    handler,
                    ctx: ctx.clone(),
                },
            ));
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Run the worker until `cancel` fires, then drain in-flight jobs and return.
/// No-op when no jobs are registered.
pub async fn run_worker(
    pool: deadpool_lapin::Pool,
    registry: JobRegistry,
    platform_pool: PgPool,
    prefetch: u16,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    if registry.is_empty() {
        return Ok(());
    }

    let conn = pool.get().await?;
    let setup = conn.create_channel().await?;
    declare_exchanges(&setup).await?;

    let tracker = TaskTracker::new();
    for (name, entry) in registry.entries {
        let channel = conn.create_channel().await?;
        declare_job_queues(&channel, &name).await?;
        channel
            .basic_qos(prefetch, BasicQosOptions::default())
            .await?;
        let consumer = channel
            .basic_consume(
                &main_queue(&name),
                &format!("junius-worker-{name}"),
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await?;
        let platform_pool = platform_pool.clone();
        let cancel = cancel.clone();
        tracker.spawn(consume_loop(
            channel,
            consumer,
            entry,
            platform_pool,
            cancel,
        ));
    }
    tracker.close();
    tracker.wait().await;
    Ok(())
}

fn main_queue(name: &str) -> String {
    format!("junius.jobs.{name}")
}

fn dead_letter_queue(name: &str) -> String {
    format!("junius.jobs.{name}.dlq")
}

/// Declare a job's durable main queue (with a dead-letter route) + its DLQ.
/// Idempotent; also called by tests to pre-create topology before enqueuing.
pub async fn declare_job_queues(channel: &Channel, name: &str) -> anyhow::Result<()> {
    let mut args = FieldTable::default();
    args.insert(
        "x-dead-letter-exchange".into(),
        AMQPValue::LongString(JOBS_DLX.into()),
    );
    args.insert(
        "x-dead-letter-routing-key".into(),
        AMQPValue::LongString(name.into()),
    );
    let durable = QueueDeclareOptions {
        durable: true,
        ..Default::default()
    };
    channel
        .queue_declare(&main_queue(name), durable, args)
        .await?;
    channel
        .queue_bind(
            &main_queue(name),
            JOBS_EXCHANGE,
            name,
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await?;
    channel
        .queue_declare(&dead_letter_queue(name), durable, FieldTable::default())
        .await?;
    channel
        .queue_bind(
            &dead_letter_queue(name),
            JOBS_DLX,
            name,
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await?;
    Ok(())
}

async fn consume_loop(
    channel: Channel,
    mut consumer: lapin::Consumer,
    entry: Entry,
    platform_pool: PgPool,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            next = consumer.next() => match next {
                Some(Ok(delivery)) => {
                    if let Err(e) = process(&channel, &entry, &platform_pool, delivery).await {
                        tracing::error!(error = %e, "job processing failed");
                    }
                }
                Some(Err(e)) => tracing::error!(error = %e, "job consumer error"),
                None => break,
            },
        }
    }
}

/// Process one delivery: mark running, dispatch, then ack + record the outcome
/// (retry by re-publish, or dead-letter on the final attempt).
async fn process(
    channel: &Channel,
    entry: &Entry,
    platform_pool: &PgPool,
    delivery: lapin::message::Delivery,
) -> anyhow::Result<()> {
    let envelope: JobEnvelope = match serde_json::from_slice(&delivery.data) {
        Ok(e) => e,
        Err(e) => {
            // Poison message: drop it (ack) so it doesn't wedge the queue.
            tracing::error!(error = %e, "undecodable job envelope; dropping");
            delivery.acker.ack(BasicAckOptions::default()).await?;
            return Ok(());
        }
    };

    sqlx::query("UPDATE meta.job_run SET status = 'running', started_at = now(), attempt = $2 WHERE id = $1")
        .bind(envelope.run_id)
        .bind(envelope.attempt)
        .execute(platform_pool)
        .await?;

    let resources = PluginResources::from_ctx(&entry.ctx, None);
    let result = entry
        .handler
        .dispatch(envelope.payload.clone(), resources)
        .await;

    match result {
        Ok(()) => {
            delivery.acker.ack(BasicAckOptions::default()).await?;
            sqlx::query(
                "UPDATE meta.job_run SET status = 'completed', finished_at = now() WHERE id = $1",
            )
            .bind(envelope.run_id)
            .execute(platform_pool)
            .await?;
        }
        Err(e) => {
            let next_attempt = envelope.attempt + 1;
            let error = e.to_string();
            if next_attempt < MAX_ATTEMPTS {
                let retry = JobEnvelope {
                    attempt: next_attempt,
                    ..envelope.clone()
                };
                publish(channel, JOBS_EXCHANGE, &envelope.job_name, &retry).await?;
                delivery.acker.ack(BasicAckOptions::default()).await?;
                sqlx::query("UPDATE meta.job_run SET status = 'enqueued', attempt = $2, error = $3 WHERE id = $1")
                    .bind(envelope.run_id)
                    .bind(next_attempt)
                    .bind(&error)
                    .execute(platform_pool)
                    .await?;
            } else {
                publish(channel, JOBS_DLX, &envelope.job_name, &envelope).await?;
                delivery.acker.ack(BasicAckOptions::default()).await?;
                sqlx::query("UPDATE meta.job_run SET status = 'failed', attempt = $2, finished_at = now(), error = $3 WHERE id = $1")
                    .bind(envelope.run_id)
                    .bind(next_attempt)
                    .bind(&error)
                    .execute(platform_pool)
                    .await?;
            }
        }
    }
    Ok(())
}

async fn publish(
    channel: &Channel,
    exchange: &str,
    routing_key: &str,
    envelope: &JobEnvelope,
) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec(envelope)?;
    channel
        .basic_publish(
            exchange,
            routing_key,
            BasicPublishOptions::default(),
            &bytes,
            BasicProperties::default(),
        )
        .await?
        .await?;
    Ok(())
}
