//! Integration test for storage URL negotiation against Postgres + `MinIO`. One
//! physical bucket advertises presign support (clients get provider URLs); a
//! second disables it (clients get juniusd-mediated URLs served by the host
//! `/api/storage` endpoints). Both round-trip bytes + record `platform.object`.
//! Skips without Docker.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::Request;
use junius_manifest::{PhysicalBucket, StorageConfig};
use junius_sdk::{BucketName, ObjectStore, PluginStorage, UrlSigner};
use platform::storage::build_object_stores;
use platform::storage::http::{HostUrlSigner, StorageHttpState, router};
use platform::storage::token::TokenSigner;
use sqlx::PgPool;
use testcontainers_modules::minio::MinIO;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tower::ServiceExt;

const HOST_MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_users.up.sql"),
    include_str!("../migrations/0002_sessions.up.sql"),
    include_str!("../migrations/0003_groups_roles_memberships.up.sql"),
    include_str!("../migrations/0004_resource_principal_share.up.sql"),
    include_str!("../migrations/0005_user_can_access.up.sql"),
    include_str!("../migrations/0006_meta_migrations.up.sql"),
    include_str!("../migrations/0007_audit_event.up.sql"),
    include_str!("../migrations/0008_authz_functions.up.sql"),
    include_str!("../migrations/0009_job_run.up.sql"),
    include_str!("../migrations/0010_object.up.sql"),
    include_str!("../migrations/0011_user_locale.up.sql"),
];

struct Prov;
impl BucketName for Prov {
    fn logical(&self) -> &'static str {
        "prov"
    }
}
struct Med;
impl BucketName for Med {
    fn logical(&self) -> &'static str {
        "med"
    }
}

#[allow(clippy::fn_params_excessive_bools)]
fn bucket(endpoint: &str, name: &str, presign: bool) -> PhysicalBucket {
    PhysicalBucket {
        endpoint: endpoint.to_string(),
        region: "us-east-1".to_string(),
        bucket: name.to_string(),
        access_key: "minioadmin".to_string(),
        secret_key: "minioadmin".to_string(),
        use_path_style: true,
        public: false,
        presigned_put: presign,
        presigned_get: presign,
        public_get: false,
    }
}

#[tokio::test]
async fn url_negotiation_provider_and_mediated() {
    let pg = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping storage_urls_minio: Docker unavailable ({e})");
            return;
        }
    };
    let minio = match MinIO::default().start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping storage_urls_minio: MinIO unavailable ({e})");
            return;
        }
    };
    let pg_port = pg.get_host_port_ipv4(5432).await.unwrap();
    let minio_port = minio.get_host_port_ipv4(9000).await.unwrap();
    let endpoint = format!("http://127.0.0.1:{minio_port}");
    let pool = PgPool::connect(&format!(
        "postgres://postgres:postgres@127.0.0.1:{pg_port}/postgres"
    ))
    .await
    .unwrap();
    for sql in HOST_MIGRATIONS {
        sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    }

    let mut buckets = BTreeMap::new();
    buckets.insert(
        "provider".to_string(),
        bucket(&endpoint, "prov-bucket", true),
    );
    buckets.insert(
        "mediated".to_string(),
        bucket(&endpoint, "med-bucket", false),
    );
    let mut mapping = BTreeMap::new();
    mapping.insert("hello:prov".to_string(), "provider".to_string());
    mapping.insert("hello:med".to_string(), "mediated".to_string());
    let cfg = StorageConfig {
        buckets,
        mapping,
        token_secret: Some("test-secret".to_string()),
    };

    let stores: HashMap<String, Arc<dyn ObjectStore>> = build_object_stores(&cfg).await.unwrap();
    let token_signer = TokenSigner::new("test-secret");
    let signer: Arc<dyn UrlSigner> = Arc::new(HostUrlSigner::new(token_signer.clone()));
    let plugin_mapping = [
        ("prov".to_string(), "provider".to_string()),
        ("med".to_string(), "mediated".to_string()),
    ]
    .into_iter()
    .collect();
    let storage = PluginStorage::new(
        stores.clone(),
        plugin_mapping,
        "hello",
        &["storage.read", "storage.write"],
        Some(pool.clone()),
        Some(signer),
    );

    // --- provider-presigned path: client talks straight to MinIO ---
    let http = reqwest::Client::new();
    let prov = storage.bucket(Prov).unwrap();
    let target = prov
        .upload_url("a.txt", "text/plain", Duration::from_secs(60))
        .await
        .unwrap();
    assert!(
        target.url.starts_with("http"),
        "provider URL: {}",
        target.url
    );
    let put = http
        .put(&target.url)
        .body("provider-bytes")
        .send()
        .await
        .unwrap();
    assert!(put.status().is_success(), "presigned PUT: {}", put.status());

    let get_url = prov
        .download_url("a.txt", Duration::from_secs(60))
        .await
        .unwrap();
    let body = http
        .get(&get_url)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert_eq!(&body[..], b"provider-bytes");

    // The object row exists for the provider upload.
    let exists: i64 = sqlx::query_scalar("SELECT count(*) FROM platform.object WHERE id = $1")
        .bind(target.object_id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(exists, 1);

    // --- juniusd-mediated path: URLs point at /api/storage, served by the host ---
    let app = router(StorageHttpState {
        stores: Arc::new(stores),
        signer: token_signer,
        platform_pool: pool.clone(),
    });
    let med = storage.bucket(Med).unwrap();
    let target = med
        .upload_url("b.txt", "text/plain", Duration::from_secs(60))
        .await
        .unwrap();
    assert!(
        target.url.starts_with("/api/storage/upload?token="),
        "mediated URL: {}",
        target.url
    );
    let put = app
        .clone()
        .oneshot(
            Request::put(&target.url)
                .header("content-type", "text/plain")
                .body(Body::from("mediated-bytes"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put.status(), 200);

    let get_url = med
        .download_url("b.txt", Duration::from_secs(60))
        .await
        .unwrap();
    assert!(get_url.starts_with("/api/storage/download?token="));
    let resp = app
        .oneshot(Request::get(&get_url).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    assert_eq!(&body[..], b"mediated-bytes");

    // The mediated upload finalized the row (size set by the host endpoint).
    let size: Option<i64> =
        sqlx::query_scalar("SELECT size_bytes FROM platform.object WHERE id = $1")
            .bind(target.object_id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(size, Some(i64::try_from("mediated-bytes".len()).unwrap()));
}
