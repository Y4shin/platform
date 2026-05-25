//! juniusd-mediated storage endpoints (M10 Stage 7), mounted under
//! `/api/storage`. Used when the physical bucket's provider lacks a presign /
//! public capability: the SDK hands the client a signed juniusd URL instead of a
//! provider URL, and these handlers stream the bytes to/from the backend. The
//! token (op/bucket/key/expiry, HMAC-signed) is the access grant.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use junius_sdk::{ObjectStore, UrlSigner};
use sqlx::PgPool;

use super::token::TokenSigner;

/// State shared by the storage endpoints.
#[derive(Clone)]
pub struct StorageHttpState {
    pub stores: Arc<HashMap<String, Arc<dyn ObjectStore>>>,
    pub signer: TokenSigner,
    pub platform_pool: PgPool,
}

/// Host [`UrlSigner`]: produces relative `/api/storage/*` URLs with a signed
/// token (resolved against the host origin by the client).
pub struct HostUrlSigner {
    signer: TokenSigner,
}

impl HostUrlSigner {
    #[must_use]
    pub fn new(signer: TokenSigner) -> Self {
        Self { signer }
    }
}

impl UrlSigner for HostUrlSigner {
    fn upload_url(&self, physical_bucket: &str, key: &str, ttl_secs: u32) -> String {
        let token = self.signer.sign('p', physical_bucket, key, ttl_secs);
        format!("/api/storage/upload?token={token}")
    }
    fn download_url(&self, physical_bucket: &str, key: &str, ttl_secs: u32) -> String {
        let token = self.signer.sign('g', physical_bucket, key, ttl_secs);
        format!("/api/storage/download?token={token}")
    }
}

pub fn router(state: StorageHttpState) -> Router {
    Router::new()
        .route("/api/storage/upload", put(upload).post(upload))
        .route("/api/storage/download", get(download))
        .with_state(state)
}

#[derive(serde::Deserialize)]
struct TokenQuery {
    token: String,
}

/// Stream an upload to the backend and finalize the `platform.object` row.
async fn upload(
    State(st): State<StorageHttpState>,
    Query(q): Query<TokenQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let token = match st.signer.verify(&q.token) {
        Ok(t) if t.op == 'p' => t,
        _ => return (StatusCode::FORBIDDEN, "invalid upload token").into_response(),
    };
    let Some(store) = st.stores.get(&token.bucket) else {
        return (StatusCode::NOT_FOUND, "unknown bucket").into_response();
    };
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();
    let size = i64::try_from(body.len()).unwrap_or(i64::MAX);
    if let Err(e) = store.put(&token.key, body.to_vec(), &content_type).await {
        return (StatusCode::BAD_GATEWAY, format!("upload failed: {e}")).into_response();
    }
    // Finalize the row created when the URL was issued (size/content-type now known).
    let _ = sqlx::query(
        "UPDATE platform.object SET size_bytes = $1, content_type = $2 \
         WHERE physical_bucket = $3 AND object_key = $4",
    )
    .bind(size)
    .bind(&content_type)
    .bind(&token.bucket)
    .bind(&token.key)
    .execute(&st.platform_pool)
    .await;
    StatusCode::OK.into_response()
}

/// Stream a download from the backend.
async fn download(State(st): State<StorageHttpState>, Query(q): Query<TokenQuery>) -> Response {
    let token = match st.signer.verify(&q.token) {
        Ok(t) if t.op == 'g' => t,
        _ => return (StatusCode::FORBIDDEN, "invalid download token").into_response(),
    };
    let Some(store) = st.stores.get(&token.bucket) else {
        return (StatusCode::NOT_FOUND, "unknown bucket").into_response();
    };
    match store.get(&token.key).await {
        Ok(bytes) => {
            let content_type: Option<String> = sqlx::query_scalar(
                "SELECT content_type FROM platform.object \
                 WHERE physical_bucket = $1 AND object_key = $2",
            )
            .bind(&token.bucket)
            .bind(&token.key)
            .fetch_optional(&st.platform_pool)
            .await
            .ok()
            .flatten();
            let ct = content_type.unwrap_or_else(|| "application/octet-stream".to_string());
            ([(header::CONTENT_TYPE, ct)], bytes).into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "object not found").into_response(),
    }
}
