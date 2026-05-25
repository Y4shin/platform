//! Integration test for object storage against ephemeral Postgres + `MinIO`:
//! put/get/delete through `ObjectStore`, plugin-scoped key isolation, a
//! `platform.object` row recorded via the SECURITY DEFINER function, and a
//! cross-plugin FK to `platform.object(id)` resolving. Skips without Docker.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]

use std::collections::HashMap;

use junius_manifest::{PhysicalBucket, StorageConfig};
use junius_sdk::{BucketName, PluginStorage};
use platform::storage::build_object_stores;
use sqlx::PgPool;
use testcontainers_modules::minio::MinIO;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

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
];

/// A standalone logical-bucket marker (the macro generates these per plugin).
struct Attachments;
impl BucketName for Attachments {
    fn logical(&self) -> &'static str {
        "attachments"
    }
}

fn storage_config(endpoint: String) -> StorageConfig {
    let mut buckets = std::collections::BTreeMap::new();
    buckets.insert(
        "main".to_string(),
        PhysicalBucket {
            endpoint,
            region: "us-east-1".to_string(),
            bucket: "junius-test".to_string(),
            access_key: "minioadmin".to_string(),
            secret_key: "minioadmin".to_string(),
            use_path_style: true,
            public: false,
            presigned_put: true,
            presigned_get: true,
            public_get: false,
        },
    );
    let mut mapping = std::collections::BTreeMap::new();
    mapping.insert("hello:attachments".to_string(), "main".to_string());
    mapping.insert("other:attachments".to_string(), "main".to_string());
    StorageConfig {
        buckets,
        mapping,
        token_secret: None,
    }
}

fn plugin_storage(
    stores: &HashMap<String, std::sync::Arc<dyn junius_sdk::ObjectStore>>,
    plugin: &'static str,
    pool: &PgPool,
) -> PluginStorage {
    let mapping = [("attachments".to_string(), "main".to_string())]
        .into_iter()
        .collect();
    PluginStorage::new(
        stores.clone(),
        mapping,
        plugin,
        &["storage.read", "storage.write"],
        Some(pool.clone()),
        None,
    )
}

#[tokio::test]
async fn storage_round_trip_isolation_and_object_row() {
    let pg = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping storage_minio: Docker unavailable ({e})");
            return;
        }
    };
    let minio = match MinIO::default().start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping storage_minio: MinIO unavailable ({e})");
            return;
        }
    };
    let pg_port = pg.get_host_port_ipv4(5432).await.unwrap();
    let minio_port = minio.get_host_port_ipv4(9000).await.unwrap();
    let pool = PgPool::connect(&format!(
        "postgres://postgres:postgres@127.0.0.1:{pg_port}/postgres"
    ))
    .await
    .unwrap();
    for sql in HOST_MIGRATIONS {
        sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    }

    let cfg = storage_config(format!("http://127.0.0.1:{minio_port}"));
    let stores = build_object_stores(&cfg).await.unwrap();

    let hello = plugin_storage(&stores, "hello", &pool);
    let other = plugin_storage(&stores, "other", &pool);

    // Put + get round-trips through the bucket handle.
    let bucket = hello.bucket(Attachments).unwrap();
    let id = bucket
        .put("notes/1/file.txt", b"hello world".to_vec(), "text/plain")
        .await
        .unwrap();
    assert_eq!(
        bucket.get("notes/1/file.txt").await.unwrap(),
        b"hello world"
    );

    // The recorded object row carries the plugin-scoped key.
    let (plugin, key): (String, String) =
        sqlx::query_as("SELECT plugin, object_key FROM platform.object WHERE id = $1")
            .bind(id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(plugin, "hello");
    assert_eq!(key, "hello/notes/1/file.txt");

    // Isolation: another plugin's handle scopes to `other/...` and can't read it.
    assert!(
        other
            .bucket(Attachments)
            .unwrap()
            .get("notes/1/file.txt")
            .await
            .is_err(),
        "plugin `other` must not read plugin `hello`'s object"
    );

    // A cross-plugin FK into platform.object(id) resolves.
    sqlx::raw_sql("CREATE TABLE ref_obj (oid UUID REFERENCES platform.object(id))")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO ref_obj (oid) VALUES ($1)")
        .bind(id.0)
        .execute(&pool)
        .await
        .unwrap();

    // Delete removes a (non-referenced) object + its row.
    let id2 = bucket
        .put("notes/2/file.txt", b"second".to_vec(), "text/plain")
        .await
        .unwrap();
    bucket.delete("notes/2/file.txt").await.unwrap();
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM platform.object WHERE id = $1")
        .bind(id2.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, 0, "delete should remove the platform.object row");
}
