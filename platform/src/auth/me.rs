//! `GET /api/me` — the current user as JSON, or 401 when unauthenticated.
//! `POST /api/me/locale` — persist the caller's locale preference.
//! `POST /api/me/refresh-groups` — re-pull the caller's OIDC `groups` claim
//! from the `IdP`'s `userinfo` endpoint and reconcile their Junius
//! `managed_by='oidc'` group memberships. Use case: an Authentik webhook
//! → `n8n` → POST here flow that doesn't wait for the next user login.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use junius_sdk::{CurrentUser, Locale, MaybeUser};
use serde::{Deserialize, Serialize};

use crate::auth::AuthState;
use crate::auth::crypto;
use crate::auth::oidc_groups;

pub async fn handler(MaybeUser(user): MaybeUser) -> Response {
    match user {
        Some(user) => Json(user).into_response(),
        None => (StatusCode::UNAUTHORIZED, "not authenticated").into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct LocaleUpdate {
    /// Locale code (e.g. `"en"`, `"de"`). `null` clears the preference, so the
    /// host falls back to `Accept-Language` and then the deployment default.
    pub locale: Option<String>,
}

/// `POST /api/me/locale` — update the caller's stored locale preference.
/// Validates against [`Locale::from_code`]; rejects unknown codes with 400.
pub async fn update_locale(
    State(state): State<AuthState>,
    CurrentUser(user): CurrentUser,
    Json(body): Json<LocaleUpdate>,
) -> Response {
    let stored: Option<String> = match body.locale {
        Some(code) => match Locale::from_code(&code) {
            Some(locale) => Some(locale.code().to_string()),
            None => return (StatusCode::BAD_REQUEST, "unknown locale").into_response(),
        },
        None => None,
    };
    match sqlx::query("UPDATE platform.user SET locale = $1, updated_at = now() WHERE id = $2")
        .bind(&stored)
        .bind(user.id.0)
        .execute(&state.pool)
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!(error = %e, "failed to persist user locale");
            (StatusCode::INTERNAL_SERVER_ERROR, "failed").into_response()
        }
    }
}

/// Response body for `POST /api/me/refresh-groups`. Reports what the
/// reconciliation actually did so a webhook caller can verify their trigger
/// landed (and isn't blocked waiting on a re-login).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshGroupsResponse {
    pub oidc_groups_claimed: usize,
    pub memberships_added: usize,
    pub memberships_reaped: usize,
}

/// `POST /api/me/refresh-groups` — re-fetch the caller's OIDC `groups`
/// claim from the `IdP`'s `userinfo` endpoint and reconcile their
/// `managed_by='oidc'` Junius group memberships against it. The caller
/// must be authenticated; no other gate. Returns the reconciler's
/// counters as JSON.
///
/// Why a raw `reqwest` call to `userinfo` (not `openidconnect`'s typed API):
/// `CoreClient::user_info` returns `UserInfoClaims<EmptyAdditionalClaims,
/// _>` which doesn't expose the `groups` claim without changing the
/// `AdditionalClaims` type parameter (a much bigger refactor across the
/// auth module). The raw call costs us nothing — the access token is
/// already trusted, the URL was discovered by `build_client`, and JSON
/// extraction is a one-liner.
pub async fn refresh_groups(
    State(state): State<AuthState>,
    CurrentUser(user): CurrentUser,
) -> Response {
    let Some(userinfo_url) = state.oidc_userinfo_url.clone() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "OIDC userinfo endpoint not configured",
        )
            .into_response();
    };

    // The caller's most recent active session carries the stored access
    // token. Decrypt and use it to call userinfo. If the token has expired
    // we surface 401 + a re-login hint; the alternative (silently doing
    // nothing) leaves the operator confused.
    let row = match sqlx::query_as::<_, (Option<Vec<u8>>,)>(
        "SELECT oidc_tokens FROM platform.session \
         WHERE user_id = $1 AND expires_at > now() \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(user.id.0)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (StatusCode::UNAUTHORIZED, "no active session").into_response();
        }
        Err(e) => {
            tracing::error!(error = %e, "session lookup failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "session lookup failed").into_response();
        }
    };
    let Some(token_blob) = row.0 else {
        return (
            StatusCode::UNAUTHORIZED,
            "no stored OIDC tokens on this session; please re-login",
        )
            .into_response();
    };
    let Some(access_bytes) = crypto::decrypt(&state.cipher, &token_blob) else {
        tracing::error!("decrypting stored OIDC tokens failed");
        return (StatusCode::INTERNAL_SERVER_ERROR, "token decryption failed").into_response();
    };
    let access_token = String::from_utf8_lossy(&access_bytes).into_owned();

    // userinfo HTTP GET. Bearer auth, JSON response. A 401 from the IdP
    // means the access token has expired — the caller needs to re-login.
    let client = reqwest::Client::new();
    let resp = match client
        .get(&userinfo_url)
        .bearer_auth(&access_token)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "userinfo request failed");
            return (StatusCode::BAD_GATEWAY, "userinfo request failed").into_response();
        }
    };
    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return (
            StatusCode::UNAUTHORIZED,
            "OIDC access token expired; please re-login",
        )
            .into_response();
    }
    if !resp.status().is_success() {
        return (
            StatusCode::BAD_GATEWAY,
            format!("userinfo returned {}", resp.status()),
        )
            .into_response();
    }
    let body: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "userinfo response not JSON");
            return (StatusCode::BAD_GATEWAY, "userinfo response not JSON").into_response();
        }
    };
    let oidc_groups: Vec<String> = body
        .get(oidc_groups::DEFAULT_GROUPS_CLAIM)
        .and_then(serde_json::Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let summary =
        match oidc_groups::reconcile_memberships(&state.pool, user.id.0, &oidc_groups).await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %e, "OIDC reconcile failed");
                return (StatusCode::INTERNAL_SERVER_ERROR, "reconcile failed").into_response();
            }
        };
    Json(RefreshGroupsResponse {
        oidc_groups_claimed: summary.oidc_groups_claimed,
        memberships_added: summary.mappings_applied,
        memberships_reaped: summary.memberships_reaped,
    })
    .into_response()
}
