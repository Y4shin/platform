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
pub mod oidc_groups;
pub mod session;
pub mod sessions;
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
    /// Discovered userinfo endpoint URL — used by the M18 refresh-groups
    /// REST handler to GET fresh claims with the caller's access token.
    pub(crate) oidc_userinfo_url: Option<String>,
    /// Optional bearer token gating the admin OIDC-resync RPC. `None` ⇒ the
    /// admin sweep is disabled (every `ResyncOidcGroups` call is rejected).
    pub(crate) admin_api_token: Option<String>,
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
        let (oidc, oidc_userinfo_url) = match oidc::build_client(
            &cfg.oidc_issuer,
            &cfg.oidc_client_id,
            &cfg.oidc_client_secret,
            redirect_uri,
        )
        .await
        {
            Ok((client, userinfo_url)) => (Some(Arc::new(client)), userinfo_url),
            Err(e) => {
                tracing::warn!(error = %e, "OIDC discovery failed; /api/auth/login disabled");
                (None, None)
            }
        };
        Self {
            cipher: crypto::cipher_from_key(&cfg.session_encryption_key),
            cookie_key: Key::from(
                crypto::cookie_key_material(&cfg.session_encryption_key).as_slice(),
            ),
            oidc,
            oidc_userinfo_url,
            admin_api_token: cfg.admin_api_token.clone(),
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
            oidc_userinfo_url: None,
            admin_api_token: None,
            cookie_secure: false,
            session_ttl_secs: SESSION_TTL_SECS,
            pool,
        }
    }

    /// Test constructor variant that also sets the admin API token, so
    /// integration tests can exercise the admin OIDC-resync RPC.
    #[must_use]
    pub fn for_test_with_admin_token(
        pool: PgPool,
        session_encryption_key: &str,
        admin_api_token: &str,
    ) -> Self {
        Self {
            admin_api_token: Some(admin_api_token.to_string()),
            ..Self::for_test(pool, session_encryption_key)
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
