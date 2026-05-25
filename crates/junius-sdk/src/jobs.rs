//! Vendor-neutral background jobs (M10 Stage 5). A plugin defines a [`Job`]
//! payload type, enqueues it via [`Jobs::enqueue`], and registers a handler via
//! [`Plugin::jobs`](crate::plugin::Plugin::jobs). The host supplies the concrete
//! [`JobBackend`] (`RabbitMQ` via lapin) — no broker crate appears in `junius-sdk`.
//!
//! Each enqueue records a row in `meta.job_run` (durable observability; `RabbitMQ`
//! owns the actual queue) and publishes a [`JobEnvelope`] keyed by the job name.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use serde::de::DeserializeOwned;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::PluginError;
use crate::resources::{PluginResources, require_capability};

/// A background job payload. `NAME` is the globally-unique routing key,
/// conventionally `"<plugin>.<job>"`.
pub trait Job: Serialize + DeserializeOwned + Send + Sync + 'static {
    const NAME: &'static str;
}

/// The wire envelope published to the broker: enough for the worker to update the
/// `meta.job_run` row and deserialize the typed payload.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobEnvelope {
    pub run_id: Uuid,
    pub job_name: String,
    pub attempt: i32,
    pub payload: serde_json::Value,
}

/// Backend trait the host implements (RabbitMQ). Publishes an already-serialized
/// envelope under `routing_key` (the job's `NAME`).
#[async_trait]
pub trait JobBackend: Send + Sync {
    async fn publish(&self, routing_key: &str, payload: Vec<u8>) -> Result<(), JobError>;
}

/// Failure enqueuing or publishing a job.
#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error("job backend error: {0}")]
    Backend(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Per-plugin jobs handle. Cheap to clone. Built by the host in `build_ctx`.
#[derive(Clone)]
pub struct Jobs {
    backend: Option<Arc<dyn JobBackend>>,
    platform_pool: Option<PgPool>,
    plugin_name: &'static str,
    capabilities: &'static [&'static str],
}

impl std::fmt::Debug for Jobs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Jobs")
            .field("plugin_name", &self.plugin_name)
            .field("capabilities", &self.capabilities)
            .field("backend", &self.backend.is_some())
            .field("has_pool", &self.platform_pool.is_some())
            .finish()
    }
}

impl Jobs {
    /// Build a jobs handle. `backend`/`platform_pool` are `None` when the
    /// deployment configured no `[config.jobs]`.
    #[must_use]
    pub fn new(
        backend: Option<Arc<dyn JobBackend>>,
        platform_pool: Option<PgPool>,
        plugin_name: &'static str,
        capabilities: &'static [&'static str],
    ) -> Self {
        Self {
            backend,
            platform_pool,
            plugin_name,
            capabilities,
        }
    }

    /// A jobs handle with no backend (caller-less/default contexts and tests);
    /// `enqueue` errors after the capability check.
    #[must_use]
    pub fn disabled(plugin_name: &'static str, capabilities: &'static [&'static str]) -> Self {
        Self::new(None, None, plugin_name, capabilities)
    }

    /// Enqueue `payload`. Requires the `job.enqueue` capability; records a
    /// `meta.job_run` row (status `enqueued`); then publishes the envelope.
    /// Returns the `job_run` id.
    pub async fn enqueue<J: Job>(&self, payload: J) -> Result<Uuid, PluginError> {
        require_capability(self.capabilities, "job.enqueue")?;
        let (Some(backend), Some(pool)) = (&self.backend, &self.platform_pool) else {
            return Err(PluginError::External(anyhow::anyhow!(
                "job.enqueue: no job backend configured for this deployment"
            )));
        };

        let payload_json = serde_json::to_value(&payload).map_err(|e| {
            PluginError::External(anyhow::anyhow!("serialize job {}: {e}", J::NAME))
        })?;
        let run_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO meta.job_run (id, job_name, plugin, status, attempt, payload) \
             VALUES ($1, $2, $3, 'enqueued', 0, $4)",
        )
        .bind(run_id)
        .bind(J::NAME)
        .bind(self.plugin_name)
        .bind(&payload_json)
        .execute(pool)
        .await?;

        let envelope = JobEnvelope {
            run_id,
            job_name: J::NAME.to_string(),
            attempt: 0,
            payload: payload_json,
        };
        let bytes = serde_json::to_vec(&envelope)
            .map_err(|e| PluginError::External(anyhow::anyhow!("serialize envelope: {e}")))?;
        backend
            .publish(J::NAME, bytes)
            .await
            .map_err(|e| PluginError::External(anyhow::Error::new(e)))?;
        Ok(run_id)
    }
}

/// The erased future a [`JobHandler`] returns when dispatched by the worker.
pub type JobFuture = Pin<Box<dyn Future<Output = Result<(), PluginError>> + Send>>;

/// A registered handler for one [`Job`] type. Built by plugins via
/// [`JobHandler::new`]; dispatched by the host worker with the plugin's own
/// (caller-less) [`PluginResources`].
#[derive(Clone)]
pub struct JobHandler {
    name: &'static str,
    run: Arc<dyn Fn(serde_json::Value, PluginResources) -> JobFuture + Send + Sync>,
}

impl JobHandler {
    /// Register `handler` for job `J`. The worker deserializes the envelope
    /// payload into `J` before calling it.
    #[must_use]
    pub fn new<J, F, Fut>(handler: F) -> Self
    where
        J: Job,
        F: Fn(J, PluginResources) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), PluginError>> + Send + 'static,
    {
        let handler = Arc::new(handler);
        let run = Arc::new(
            move |value: serde_json::Value, resources: PluginResources| {
                let handler = handler.clone();
                Box::pin(async move {
                    let job = serde_json::from_value::<J>(value).map_err(|e| {
                        PluginError::External(anyhow::anyhow!("deserialize job {}: {e}", J::NAME))
                    })?;
                    handler(job, resources).await
                }) as JobFuture
            },
        );
        Self { name: J::NAME, run }
    }

    /// The job name this handler is registered for (its routing key).
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Dispatch the handler with a JSON payload + the plugin's resources.
    #[must_use]
    pub fn dispatch(&self, payload: serde_json::Value, resources: PluginResources) -> JobFuture {
        (self.run)(payload, resources)
    }
}

impl std::fmt::Debug for JobHandler {
    #[allow(
        clippy::missing_fields_in_debug,
        reason = "the `run` field is an erased handler closure with no useful Debug"
    )]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobHandler")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Serialize, serde::Deserialize)]
    struct Ping;
    impl Job for Ping {
        const NAME: &'static str = "test.ping";
    }

    #[tokio::test]
    async fn enqueue_requires_capability() {
        let jobs = Jobs::disabled("test", &[]);
        assert!(matches!(
            jobs.enqueue(Ping).await,
            Err(PluginError::CapabilityNotDeclared("job.enqueue"))
        ));
    }

    #[tokio::test]
    async fn enqueue_without_backend_errors_after_capability() {
        // Capability present but no backend/pool configured → still an error.
        let jobs = Jobs::disabled("test", &["job.enqueue"]);
        assert!(jobs.enqueue(Ping).await.is_err());
    }
}
