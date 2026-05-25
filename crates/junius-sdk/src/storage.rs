//! Vendor-neutral object storage (M10 Stage 6). A plugin declares **logical
//! buckets** in its manifest (`[storage.buckets.<name>]`); the deployment maps
//! each to a **physical bucket** in `[config.storage]`. The SDK exposes an opaque
//! [`PluginStorage`] resolving logical → physical and a [`BucketHandle`] with
//! capability-gated `put`/`get`/`delete`; the host supplies the concrete
//! [`ObjectStore`] (aws-sdk-s3) — no S3 crate appears in `junius-sdk`.
//!
//! Every write is tracked in `platform.object` (via a `SECURITY DEFINER` host
//! function) so plugins can FK their rows to a stored object. Keys are
//! plugin-scoped (`<plugin>/<key>`) for isolation. Bucket selection is by the
//! **generated** per-plugin `Bucket` enum (implements [`BucketName`]), never a
//! raw string — removing a manifest bucket breaks compilation.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::PluginError;
use crate::resources::require_capability;

/// A logical bucket name. The generated per-plugin `Bucket` enum implements this
/// so [`PluginStorage::bucket`] is called with a compile-checked variant.
pub trait BucketName {
    fn logical(&self) -> &'static str;
}

/// Provider capability flags for a physical bucket (Stage 7 URL negotiation).
#[derive(Debug, Clone, Copy)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent provider capability + visibility flags, mirroring config"
)]
pub struct BucketCapabilities {
    pub public: bool,
    pub presigned_put: bool,
    pub presigned_get: bool,
    pub public_get: bool,
}

/// Backend trait the host implements (aws-sdk-s3). Operates on already
/// plugin-scoped keys against one physical bucket.
#[async_trait]
pub trait ObjectStore: Send + Sync {
    async fn put(&self, key: &str, body: Vec<u8>, content_type: &str) -> Result<(), StorageError>;
    async fn get(&self, key: &str) -> Result<Vec<u8>, StorageError>;
    async fn delete(&self, key: &str) -> Result<(), StorageError>;
    /// A provider-presigned PUT URL, or `None` when the provider lacks the
    /// capability (the host then mediates the upload itself).
    async fn presign_put(&self, key: &str, ttl_secs: u32) -> Result<Option<String>, StorageError>;
    /// A provider-presigned GET URL, or `None` when unsupported.
    async fn presign_get(&self, key: &str, ttl_secs: u32) -> Result<Option<String>, StorageError>;
    /// A stable public URL, or `None` when the bucket isn't publicly readable.
    fn public_url(&self, key: &str) -> Option<String>;
    /// The physical bucket's provider capabilities.
    fn capabilities(&self) -> BucketCapabilities;
    /// The configured physical bucket name (recorded in `platform.object`).
    fn physical_bucket(&self) -> &str;
}

/// Host-provided signer for **juniusd-mediated** URLs — used when the provider
/// lacks a presign/public capability. Produces short-lived, op/object-scoped
/// signed URLs the host's `/api/storage` endpoints accept.
pub trait UrlSigner: Send + Sync {
    fn upload_url(&self, physical_bucket: &str, key: &str, ttl_secs: u32) -> String;
    fn download_url(&self, physical_bucket: &str, key: &str, ttl_secs: u32) -> String;
}

/// The result of [`BucketHandle::upload_url`]: where to PUT the bytes and the
/// `platform.object` id to FK from a plugin row.
#[derive(Debug, Clone)]
pub struct UploadTarget {
    pub url: String,
    pub object_id: ObjectId,
}

/// Failure in an object-storage operation.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// The logical bucket isn't mapped to a physical bucket for this deployment.
    #[error("storage bucket not mapped: {0}")]
    BucketNotMapped(String),
    /// The object does not exist.
    #[error("object not found")]
    NotFound,
    /// The storage backend failed.
    #[error("storage backend error: {0}")]
    Backend(String),
    /// Recording the object in `platform.object` failed.
    #[error("object bookkeeping error: {0}")]
    Db(String),
}

fn ttl_secs(ttl: std::time::Duration) -> u32 {
    u32::try_from(ttl.as_secs()).unwrap_or(u32::MAX)
}

fn to_plugin_error(e: StorageError) -> PluginError {
    PluginError::External(anyhow::Error::new(e))
}

/// An object's id in `platform.object`. Plugins FK their rows into this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ObjectId(pub Uuid);

impl std::fmt::Display for ObjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Opaque per-plugin storage handle. Resolves logical buckets to physical
/// [`ObjectStore`]s and records every write in `platform.object`. Cheap to clone.
#[derive(Clone)]
pub struct PluginStorage {
    inner: Arc<StorageInner>,
}

struct StorageInner {
    /// physical bucket name → store
    stores: HashMap<String, Arc<dyn ObjectStore>>,
    /// this plugin's logical bucket → physical bucket name
    mapping: HashMap<String, String>,
    plugin_name: &'static str,
    capabilities: &'static [&'static str],
    platform_pool: Option<PgPool>,
    /// Host signer for juniusd-mediated URLs (provider-fallback).
    signer: Option<Arc<dyn UrlSigner>>,
}

impl PluginStorage {
    #[must_use]
    pub fn new(
        stores: HashMap<String, Arc<dyn ObjectStore>>,
        mapping: HashMap<String, String>,
        plugin_name: &'static str,
        capabilities: &'static [&'static str],
        platform_pool: Option<PgPool>,
        signer: Option<Arc<dyn UrlSigner>>,
    ) -> Self {
        Self {
            inner: Arc::new(StorageInner {
                stores,
                mapping,
                plugin_name,
                capabilities,
                platform_pool,
                signer,
            }),
        }
    }

    /// An empty storage handle (no buckets) for caller-less/default contexts.
    #[must_use]
    pub fn empty(plugin_name: &'static str, capabilities: &'static [&'static str]) -> Self {
        Self::new(
            HashMap::new(),
            HashMap::new(),
            plugin_name,
            capabilities,
            None,
            None,
        )
    }

    /// Resolve a (generated, compile-checked) logical bucket to a handle.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "the generated Bucket enum is Copy; by-value reads cleaner at call sites"
    )]
    pub fn bucket<B: BucketName>(&self, bucket: B) -> Result<BucketHandle, StorageError> {
        let logical = bucket.logical();
        let physical = self.inner.mapping.get(logical).ok_or_else(|| {
            StorageError::BucketNotMapped(format!("{}:{logical}", self.inner.plugin_name))
        })?;
        let store = self
            .inner
            .stores
            .get(physical)
            .ok_or_else(|| StorageError::BucketNotMapped(physical.clone()))?
            .clone();
        Ok(BucketHandle {
            store,
            logical,
            physical: physical.clone(),
            plugin_name: self.inner.plugin_name,
            capabilities: self.inner.capabilities,
            platform_pool: self.inner.platform_pool.clone(),
            signer: self.inner.signer.clone(),
        })
    }
}

/// A handle to one logical bucket. `put`/`delete` gate `storage.write`; `get`
/// gates `storage.read`. Keys are scoped to `<plugin>/<key>`.
#[derive(Clone)]
pub struct BucketHandle {
    store: Arc<dyn ObjectStore>,
    logical: &'static str,
    /// The deployment's physical-bucket config key — the stable identifier used
    /// in `platform.object`, signed tokens, and the host endpoint's store lookup
    /// (distinct from the provider's real bucket name).
    physical: String,
    plugin_name: &'static str,
    capabilities: &'static [&'static str],
    platform_pool: Option<PgPool>,
    signer: Option<Arc<dyn UrlSigner>>,
}

impl BucketHandle {
    /// The plugin-scoped object key (`<plugin>/<key>`), enforcing isolation.
    #[must_use]
    pub fn scoped_key(&self, key: &str) -> String {
        format!("{}/{}", self.plugin_name, key)
    }

    /// Provider capabilities of the underlying physical bucket.
    #[must_use]
    pub fn capabilities(&self) -> BucketCapabilities {
        self.store.capabilities()
    }

    /// Upsert the `platform.object` row for a scoped key, returning its id.
    async fn record(
        &self,
        scoped: &str,
        content_type: &str,
        size: i64,
    ) -> Result<ObjectId, PluginError> {
        let pool = self.platform_pool.as_ref().ok_or_else(|| {
            PluginError::External(anyhow::anyhow!(
                "storage: object bookkeeping not configured"
            ))
        })?;
        let id: Uuid =
            sqlx::query_scalar("SELECT platform.record_object($1, $2, $3, $4, $5, $6, $7, $8)")
                .bind(self.plugin_name)
                .bind(self.logical)
                .bind(&self.physical)
                .bind(scoped)
                .bind(content_type)
                .bind(size)
                .bind("private")
                .bind(Option::<Uuid>::None)
                .fetch_one(pool)
                .await
                .map_err(|e| to_plugin_error(StorageError::Db(e.to_string())))?;
        Ok(ObjectId(id))
    }

    /// Store `body` at `key` and record it in `platform.object`. Returns the
    /// object id (FK target for plugin rows).
    pub async fn put(
        &self,
        key: &str,
        body: Vec<u8>,
        content_type: &str,
    ) -> Result<ObjectId, PluginError> {
        require_capability(self.capabilities, "storage.write")?;
        let scoped = self.scoped_key(key);
        let size = i64::try_from(body.len()).unwrap_or(i64::MAX);
        self.store
            .put(&scoped, body, content_type)
            .await
            .map_err(to_plugin_error)?;
        self.record(&scoped, content_type, size).await
    }

    /// A URL the client can PUT bytes to (provider-presigned when the bucket
    /// supports it, else a juniusd-mediated URL), plus the recorded object id to
    /// FK from a plugin row. The object row is created now; the bytes follow.
    pub async fn upload_url(
        &self,
        key: &str,
        content_type: &str,
        ttl: std::time::Duration,
    ) -> Result<UploadTarget, PluginError> {
        require_capability(self.capabilities, "storage.write")?;
        let scoped = self.scoped_key(key);
        let object_id = self.record(&scoped, content_type, 0).await?;
        let ttl_secs = ttl_secs(ttl);
        let url = match self
            .store
            .presign_put(&scoped, ttl_secs)
            .await
            .map_err(to_plugin_error)?
        {
            Some(url) => url,
            None => self.mediated_url(|s, b, k| s.upload_url(b, k, ttl_secs), &scoped)?,
        };
        Ok(UploadTarget { url, object_id })
    }

    /// A URL to download the object (stable public URL, else provider-presigned
    /// GET, else a juniusd-mediated URL).
    pub async fn download_url(
        &self,
        key: &str,
        ttl: std::time::Duration,
    ) -> Result<String, PluginError> {
        require_capability(self.capabilities, "storage.read")?;
        let scoped = self.scoped_key(key);
        if let Some(url) = self.store.public_url(&scoped) {
            return Ok(url);
        }
        let ttl_secs = ttl_secs(ttl);
        if let Some(url) = self
            .store
            .presign_get(&scoped, ttl_secs)
            .await
            .map_err(to_plugin_error)?
        {
            return Ok(url);
        }
        self.mediated_url(|s, b, k| s.download_url(b, k, ttl_secs), &scoped)
    }

    /// Build a juniusd-mediated URL via the host signer, erroring if no signer is
    /// configured (the provider lacked the capability and no fallback exists).
    fn mediated_url(
        &self,
        make: impl FnOnce(&dyn UrlSigner, &str, &str) -> String,
        scoped: &str,
    ) -> Result<String, PluginError> {
        let signer = self.signer.as_ref().ok_or_else(|| {
            PluginError::External(anyhow::anyhow!(
                "storage: provider lacks the capability and no host signer is configured"
            ))
        })?;
        Ok(make(signer.as_ref(), &self.physical, scoped))
    }

    /// Fetch the object at `key`.
    pub async fn get(&self, key: &str) -> Result<Vec<u8>, PluginError> {
        require_capability(self.capabilities, "storage.read")?;
        self.store
            .get(&self.scoped_key(key))
            .await
            .map_err(to_plugin_error)
    }

    /// Delete the object at `key` and its `platform.object` row (best effort).
    pub async fn delete(&self, key: &str) -> Result<(), PluginError> {
        require_capability(self.capabilities, "storage.write")?;
        let scoped = self.scoped_key(key);
        self.store.delete(&scoped).await.map_err(to_plugin_error)?;
        if let Some(pool) = &self.platform_pool {
            sqlx::query("SELECT platform.delete_object($1, $2)")
                .bind(&self.physical)
                .bind(&scoped)
                .execute(pool)
                .await
                .map_err(|e| to_plugin_error(StorageError::Db(e.to_string())))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct B;
    impl BucketName for B {
        fn logical(&self) -> &'static str {
            "attachments"
        }
    }

    #[test]
    fn unmapped_bucket_is_an_error() {
        let storage = PluginStorage::empty("hello", &["storage.read", "storage.write"]);
        assert!(matches!(
            storage.bucket(B),
            Err(StorageError::BucketNotMapped(_))
        ));
    }
}
