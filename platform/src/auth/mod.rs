//! Host authentication: OIDC login + server-side sessions + `/api/me`.
//!
//! [`AuthState`] is the shared handle (platform pool, token cipher, cookie key,
//! optional OIDC client). [`public_router`] serves the unauthenticated
//! `/api/auth/*` endpoints; the session middleware ([`session::middleware`]) and
//! `/api/me` ([`me::handler`]) are wired into the protected scope by
//! [`crate::server`].

pub mod crypto;
pub mod me;
pub mod oidc;
pub mod session;
pub mod user_query;

use std::sync::Arc;

use aes_gcm::Aes256Gcm;
use axum::Router;
use axum::routing::{get, post};
use junius_manifest::ResolvedConfig;
use openidconnect::core::CoreClient;
use sqlx::PgPool;
use tower_cookies::Key;

/// Default session lifetime: 7 days.
const SESSION_TTL_SECS: i64 = 60 * 60 * 24 * 7;

/// Shared authentication state.
#[derive(Clone)]
pub struct AuthState {
    pub(crate) pool: PgPool,
    pub(crate) cipher: Aes256Gcm,
    pub(crate) cookie_key: Key,
    pub(crate) oidc: Option<Arc<CoreClient>>,
    pub(crate) cookie_secure: bool,
    pub(crate) session_ttl_secs: i64,
}

impl AuthState {
    /// Build from resolved config. Attempts OIDC discovery; on failure logs a
    /// warning and disables login while the rest of the host still boots.
    pub async fn from_config(
        pool: PgPool,
        cfg: &ResolvedConfig,
        redirect_uri: &str,
        cookie_secure: bool,
    ) -> Self {
        let oidc = match oidc::build_client(
            &cfg.oidc_issuer,
            &cfg.oidc_client_id,
            &cfg.oidc_client_secret,
            redirect_uri,
        )
        .await
        {
            Ok(client) => Some(Arc::new(client)),
            Err(e) => {
                tracing::warn!(error = %e, "OIDC discovery failed; /api/auth/login disabled");
                None
            }
        };
        Self {
            cipher: crypto::cipher_from_key(&cfg.session_encryption_key),
            cookie_key: Key::from(
                crypto::cookie_key_material(&cfg.session_encryption_key).as_slice(),
            ),
            oidc,
            cookie_secure,
            session_ttl_secs: SESSION_TTL_SECS,
            pool,
        }
    }

    /// Construct without an OIDC client (integration tests seed sessions directly).
    #[must_use]
    pub fn for_test(pool: PgPool, session_encryption_key: &str) -> Self {
        Self {
            cipher: crypto::cipher_from_key(session_encryption_key),
            cookie_key: Key::from(crypto::cookie_key_material(session_encryption_key).as_slice()),
            oidc: None,
            cookie_secure: false,
            session_ttl_secs: SESSION_TTL_SECS,
            pool,
        }
    }
}

/// The unauthenticated `/api/auth/*` endpoints.
pub fn public_router(state: AuthState) -> Router {
    Router::new()
        .route("/api/auth/login", get(oidc::login))
        .route("/api/auth/callback", get(oidc::callback))
        .route("/api/auth/logout", post(oidc::logout))
        .with_state(state)
}
