//! OIDC Authorization-Code (+ PKCE) flow handlers: `login`, `callback`, `logout`.
//!
//! Provider-agnostic via OIDC discovery (dev uses Authentik). The short-lived
//! per-flow state (CSRF token, nonce, PKCE verifier, return path) rides in a
//! private (encrypted) cookie so the server stays stateless between login and
//! callback. On success the callback upserts `platform.user`, creates a
//! `platform.session`, and sets the session cookie.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use openidconnect::core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata};
use openidconnect::reqwest::async_http_client;
use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce, OAuth2TokenResponse,
    PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
};
use serde::{Deserialize, Serialize};
use tower_cookies::cookie::SameSite;
use tower_cookies::{Cookie, Cookies};
use uuid::Uuid;

use crate::auth::AuthState;
use crate::auth::crypto;
use crate::auth::session::SESSION_COOKIE;

const FLOW_COOKIE: &str = "junius_oidc_flow";

/// Soft-fail OIDC-groups reconciliation helper used by the login callback.
/// Errors are logged but never propagated — a failed reconciliation should
/// not block the user from logging in.
async fn reconcile_groups_on_login(pool: &sqlx::PgPool, user_id: uuid::Uuid, id_token: &str) {
    let oidc_groups = crate::auth::oidc_groups::extract_groups_from_id_token(
        id_token,
        crate::auth::oidc_groups::DEFAULT_GROUPS_CLAIM,
    );
    if let Err(e) =
        crate::auth::oidc_groups::reconcile_memberships(pool, user_id, &oidc_groups).await
    {
        tracing::warn!(error = %e, user_id = %user_id, "OIDC group reconciliation failed; carrying on");
    }
}

/// Discover the provider and build a configured client. Also returns the
/// discovered `userinfo_endpoint` URL so the host can call it directly (the
/// M18 refresh-groups REST endpoint uses a plain `reqwest` GET because the
/// typed `CoreClient::user_info` response can't expose the `groups` claim
/// without changing the `AdditionalClaims` type parameter).
pub async fn build_client(
    issuer: &str,
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
) -> anyhow::Result<(CoreClient, Option<String>)> {
    let metadata = CoreProviderMetadata::discover_async(
        IssuerUrl::new(issuer.to_string())?,
        async_http_client,
    )
    .await?;
    let userinfo_url = metadata.userinfo_endpoint().map(|u| u.to_string());
    let client = CoreClient::from_provider_metadata(
        metadata,
        ClientId::new(client_id.to_string()),
        Some(ClientSecret::new(client_secret.to_string())),
    )
    .set_redirect_uri(RedirectUrl::new(redirect_uri.to_string())?);
    Ok((client, userinfo_url))
}

#[derive(Serialize, Deserialize)]
struct FlowState {
    csrf: String,
    nonce: String,
    pkce: String,
    return_to: String,
}

#[derive(Deserialize)]
pub struct LoginQuery {
    return_to: Option<String>,
}

pub async fn login(
    State(state): State<AuthState>,
    cookies: Cookies,
    Query(query): Query<LoginQuery>,
) -> Response {
    let Some(client) = state.oidc.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "OIDC is not configured").into_response();
    };

    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let (auth_url, csrf, nonce) = client
        .authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        .add_scope(Scope::new("email".to_string()))
        .add_scope(Scope::new("profile".to_string()))
        .set_pkce_challenge(challenge)
        .url();

    let flow = FlowState {
        csrf: csrf.secret().clone(),
        nonce: nonce.secret().clone(),
        pkce: verifier.secret().clone(),
        return_to: sanitize_return_to(query.return_to.as_deref()),
    };
    let payload = serde_json::to_string(&flow).unwrap_or_default();
    let mut cookie = Cookie::new(FLOW_COOKIE, payload);
    cookie.set_http_only(true);
    cookie.set_path("/api/auth");
    cookie.set_same_site(SameSite::Lax);
    cookie.set_secure(state.cookie_secure);
    cookies.private(&state.cookie_key).add(cookie);

    Redirect::to(auth_url.as_str()).into_response()
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

pub async fn callback(
    State(state): State<AuthState>,
    cookies: Cookies,
    Query(query): Query<CallbackQuery>,
) -> Response {
    let Some(client) = state.oidc.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "OIDC is not configured").into_response();
    };
    if let Some(error) = query.error {
        return (StatusCode::BAD_REQUEST, format!("OIDC error: {error}")).into_response();
    }
    let (Some(code), Some(returned_state)) = (query.code, query.state) else {
        return (StatusCode::BAD_REQUEST, "missing code/state").into_response();
    };

    let private = cookies.private(&state.cookie_key);
    let Some(flow_cookie) = private.get(FLOW_COOKIE) else {
        return (StatusCode::BAD_REQUEST, "missing or invalid flow cookie").into_response();
    };
    let Ok(flow) = serde_json::from_str::<FlowState>(flow_cookie.value()) else {
        return (StatusCode::BAD_REQUEST, "corrupt flow cookie").into_response();
    };
    private.remove(Cookie::from(FLOW_COOKIE));

    if flow.csrf != returned_state {
        return (StatusCode::BAD_REQUEST, "state mismatch").into_response();
    }

    let token_response = match client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(PkceCodeVerifier::new(flow.pkce))
        .request_async(async_http_client)
        .await
    {
        Ok(token) => token,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("token exchange failed: {e}"),
            )
                .into_response();
        }
    };

    let Some(id_token) = token_response.id_token() else {
        return (StatusCode::BAD_GATEWAY, "provider returned no id_token").into_response();
    };
    let claims = match id_token.claims(&client.id_token_verifier(), &Nonce::new(flow.nonce)) {
        Ok(claims) => claims,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("id_token verification failed: {e}"),
            )
                .into_response();
        }
    };

    let sub = claims.subject().as_str().to_string();
    let email = claims
        .email()
        .map(|e| e.as_str().to_string())
        .unwrap_or_default();
    let display_name = claims
        .preferred_username()
        .map(|u| u.as_str().to_string())
        .or_else(|| {
            claims
                .name()
                .and_then(|n| n.get(None))
                .map(|n| n.as_str().to_string())
        })
        .unwrap_or_else(|| email.clone());

    let user_id = match upsert_user(&state.pool, &sub, &email, &display_name).await {
        Ok(id) => id,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("user upsert failed: {e}"),
            )
                .into_response();
        }
    };

    // M18 Stage C: reconcile OIDC group memberships from the `groups` claim.
    // Soft-fail at login (warn + carry on); the user can still re-trigger
    // via `POST /api/me/refresh-groups`.
    reconcile_groups_on_login(&state.pool, user_id, &id_token.to_string()).await;

    let token_blob = crypto::encrypt(
        &state.cipher,
        token_response.access_token().secret().as_bytes(),
    );
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(state.session_ttl_secs);
    let session_id = match create_session(&state.pool, user_id, expires_at, &token_blob).await {
        Ok(id) => id,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("session create failed: {e}"),
            )
                .into_response();
        }
    };

    let mut cookie = Cookie::new(SESSION_COOKIE, session_id.to_string());
    cookie.set_http_only(true);
    cookie.set_path("/");
    cookie.set_same_site(SameSite::Lax);
    cookie.set_secure(state.cookie_secure);
    cookies.add(cookie);

    Redirect::to(&flow.return_to).into_response()
}

pub async fn logout(State(state): State<AuthState>, cookies: Cookies) -> Response {
    if let Some(cookie) = cookies.get(SESSION_COOKIE) {
        if let Ok(id) = Uuid::parse_str(cookie.value()) {
            if let Err(e) = sqlx::query("DELETE FROM platform.session WHERE id = $1")
                .bind(id)
                .execute(&state.pool)
                .await
            {
                tracing::warn!(error = %e, "failed to delete session row on logout");
            }
        }
    }
    cookies.remove(Cookie::from(SESSION_COOKIE));
    (StatusCode::OK, "logged out").into_response()
}

async fn upsert_user(
    pool: &sqlx::PgPool,
    sub: &str,
    email: &str,
    display_name: &str,
) -> Result<Uuid, sqlx::Error> {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO platform.user (oidc_sub, email, display_name) VALUES ($1, $2, $3) \
         ON CONFLICT (oidc_sub) DO UPDATE \
         SET email = EXCLUDED.email, display_name = EXCLUDED.display_name, updated_at = now() \
         RETURNING id",
    )
    .bind(sub)
    .bind(email)
    .bind(display_name)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

async fn create_session(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    expires_at: chrono::DateTime<chrono::Utc>,
    oidc_tokens: &[u8],
) -> Result<Uuid, sqlx::Error> {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO platform.session (user_id, expires_at, oidc_tokens) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(user_id)
    .bind(expires_at)
    .bind(oidc_tokens)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Only permit local-path redirects (avoid open-redirect via `return_to`).
fn sanitize_return_to(return_to: Option<&str>) -> String {
    match return_to {
        Some(path) if path.starts_with('/') && !path.starts_with("//") => path.to_string(),
        _ => "/".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_rejects_non_local() {
        assert_eq!(sanitize_return_to(Some("/p/hello")), "/p/hello");
        assert_eq!(sanitize_return_to(Some("//evil.com")), "/");
        assert_eq!(sanitize_return_to(Some("https://evil.com")), "/");
        assert_eq!(sanitize_return_to(None), "/");
    }
}
