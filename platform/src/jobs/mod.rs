//! Host job queue over `RabbitMQ` (M10 Stage 5). The SDK defines the
//! [`JobBackend`](junius_sdk::JobBackend) trait; the concrete lapin/`RabbitMQ`
//! impl lives here. Jobs are published to a durable direct exchange keyed by the
//! job name; the [`worker`] declares a durable queue + dead-letter queue per
//! registered job and dispatches deliveries to plugin handlers.

pub mod audit_prune;
pub mod worker;

use std::sync::Arc;

use async_trait::async_trait;
use deadpool_lapin::{Manager, Pool};
use junius_sdk::{JobBackend, JobError};
use lapin::options::{BasicPublishOptions, ExchangeDeclareOptions};
use lapin::types::FieldTable;
use lapin::{BasicProperties, ConnectionProperties, ExchangeKind};

/// Durable direct exchange jobs are published to (routing key = job name).
pub const JOBS_EXCHANGE: &str = "junius.jobs";
/// Dead-letter exchange final-failed jobs are routed to.
pub const JOBS_DLX: &str = "junius.jobs.dlx";

/// Build a lazy connection pool for the broker (no connection until first use).
pub fn connect_pool(amqp_url: &str) -> anyhow::Result<Pool> {
    let manager = Manager::new(amqp_url, ConnectionProperties::default());
    let pool = Pool::builder(manager).max_size(8).build()?;
    Ok(pool)
}

/// Declare the durable job + dead-letter exchanges (idempotent).
pub async fn declare_exchanges(channel: &lapin::Channel) -> anyhow::Result<()> {
    for exchange in [JOBS_EXCHANGE, JOBS_DLX] {
        channel
            .exchange_declare(
                exchange,
                ExchangeKind::Direct,
                ExchangeDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await?;
    }
    Ok(())
}

/// lapin-backed [`JobBackend`]: publishes serialized envelopes to
/// [`JOBS_EXCHANGE`] under the job name as routing key.
pub struct LapinJobBackend {
    pool: Pool,
}

impl LapinJobBackend {
    #[must_use]
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl JobBackend for LapinJobBackend {
    async fn publish(&self, routing_key: &str, payload: Vec<u8>) -> Result<(), JobError> {
        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| JobError::Backend(format!("acquire connection: {e}")))?;
        let channel = conn
            .create_channel()
            .await
            .map_err(|e| JobError::Backend(format!("create channel: {e}")))?;
        channel
            .basic_publish(
                JOBS_EXCHANGE,
                routing_key,
                BasicPublishOptions::default(),
                &payload,
                BasicProperties::default(),
            )
            .await
            .map_err(|e| JobError::Backend(format!("publish: {e}")))?
            .await
            .map_err(|e| JobError::Backend(format!("publish confirm: {e}")))?;
        Ok(())
    }
}

/// Build the broker pool + a shared [`JobBackend`], declaring the exchanges
/// up-front so early enqueues land on a real exchange. Requires the broker to be
/// reachable at boot.
pub async fn build(amqp_url: &str) -> anyhow::Result<(Pool, Arc<dyn JobBackend>)> {
    let pool = connect_pool(amqp_url)?;
    let conn = pool.get().await?;
    let channel = conn.create_channel().await?;
    declare_exchanges(&channel).await?;
    let backend: Arc<dyn JobBackend> = Arc::new(LapinJobBackend::new(pool.clone()));
    Ok((pool, backend))
}
