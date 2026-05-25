//! Host object storage over S3-compatible backends (M10 Stage 6). The SDK
//! defines the vendor-neutral [`ObjectStore`](junius_sdk::ObjectStore) trait; the
//! concrete impl lives here. One client per physical bucket is built from
//! `[config.storage.buckets]`; each bucket is ensured to exist at boot.
//!
//! Backend note: the user-confirmed provider is aws-sdk-s3, but its current
//! release requires a newer rustc than this dev shell pins, so this uses the
//! plan's sanctioned fallback `rust-s3` (pure-Rust, rustls/ring) behind the
//! `ObjectStore` trait. Swapping back to aws-sdk-s3 is localized to this module.

pub mod http;
pub mod token;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use junius_manifest::{PhysicalBucket, StorageConfig};
use junius_sdk::{BucketCapabilities, ObjectStore, StorageError};
use s3::creds::Credentials;
use s3::{Bucket, BucketConfiguration, Region};

fn region_of(b: &PhysicalBucket) -> Region {
    Region::Custom {
        region: b.region.clone(),
        endpoint: b.endpoint.clone(),
    }
}

fn credentials_of(b: &PhysicalBucket) -> anyhow::Result<Credentials> {
    Credentials::new(Some(&b.access_key), Some(&b.secret_key), None, None, None)
        .map_err(|e| anyhow::anyhow!("storage credentials for {}: {e}", b.bucket))
}

/// rust-s3-backed [`ObjectStore`] for one physical bucket.
pub struct S3ObjectStore {
    bucket: Box<Bucket>,
    name: String,
    caps: BucketCapabilities,
}

#[async_trait]
impl ObjectStore for S3ObjectStore {
    async fn put(&self, key: &str, body: Vec<u8>, content_type: &str) -> Result<(), StorageError> {
        self.bucket
            .put_object_with_content_type(key, &body, content_type)
            .await
            .map_err(|e| StorageError::Backend(format!("put {key}: {e}")))?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        let resp = self
            .bucket
            .get_object(key)
            .await
            .map_err(|e| StorageError::Backend(format!("get {key}: {e}")))?;
        Ok(resp.bytes().to_vec())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.bucket
            .delete_object(key)
            .await
            .map_err(|e| StorageError::Backend(format!("delete {key}: {e}")))?;
        Ok(())
    }

    async fn presign_put(&self, key: &str, ttl_secs: u32) -> Result<Option<String>, StorageError> {
        if !self.caps.presigned_put {
            return Ok(None);
        }
        self.bucket
            .presign_put(key, ttl_secs, None, None)
            .await
            .map(Some)
            .map_err(|e| StorageError::Backend(format!("presign put {key}: {e}")))
    }

    async fn presign_get(&self, key: &str, ttl_secs: u32) -> Result<Option<String>, StorageError> {
        if !self.caps.presigned_get {
            return Ok(None);
        }
        self.bucket
            .presign_get(key, ttl_secs, None)
            .await
            .map(Some)
            .map_err(|e| StorageError::Backend(format!("presign get {key}: {e}")))
    }

    fn public_url(&self, key: &str) -> Option<String> {
        // `Bucket::url()` already includes the bucket (path-style) or encodes it
        // in the host (virtual-host style).
        self.caps
            .public_get
            .then(|| format!("{}/{}", self.bucket.url(), key))
    }

    fn capabilities(&self) -> BucketCapabilities {
        self.caps
    }

    fn physical_bucket(&self) -> &str {
        &self.name
    }
}

/// Build one [`ObjectStore`] per physical bucket and ensure each exists.
pub async fn build_object_stores(
    cfg: &StorageConfig,
) -> anyhow::Result<HashMap<String, Arc<dyn ObjectStore>>> {
    let mut stores: HashMap<String, Arc<dyn ObjectStore>> = HashMap::new();
    for (physical, b) in &cfg.buckets {
        ensure_bucket(b).await;
        let mut bucket = Bucket::new(&b.bucket, region_of(b), credentials_of(b)?)
            .map_err(|e| anyhow::anyhow!("open bucket {}: {e}", b.bucket))?;
        if b.use_path_style {
            bucket.set_path_style();
        }
        let caps = BucketCapabilities {
            public: b.public,
            presigned_put: b.presigned_put,
            presigned_get: b.presigned_get,
            public_get: b.public_get,
        };
        stores.insert(
            physical.clone(),
            Arc::new(S3ObjectStore {
                bucket,
                name: b.bucket.clone(),
                caps,
            }),
        );
    }
    Ok(stores)
}

/// Best-effort create-if-absent: an existing bucket (or a provider that rejects a
/// duplicate create) is fine; a hard failure is logged but doesn't abort boot
/// (the first real `put` will surface a misconfiguration).
async fn ensure_bucket(b: &PhysicalBucket) {
    let Ok(creds) = credentials_of(b) else {
        return;
    };
    let result = Bucket::create_with_path_style(
        &b.bucket,
        region_of(b),
        creds,
        BucketConfiguration::default(),
    )
    .await;
    match result {
        Ok(resp) if resp.success() || resp.response_code == 409 => {}
        Ok(resp) => tracing::warn!(
            bucket = %b.bucket,
            code = resp.response_code,
            "ensure bucket: unexpected response"
        ),
        Err(e) => tracing::warn!(bucket = %b.bucket, error = %e, "ensure bucket failed"),
    }
}
