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
/// The token fetch + claim extraction is the shared
/// [`oidc_groups::fetch_oidc_groups`] helper (also used by the host
/// `UserService` RPC); this handler just maps its error to an HTTP status and
/// then reconciles.
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

    let oidc_groups =
        match oidc_groups::fetch_oidc_groups(&state.pool, &state.cipher, &userinfo_url, user.id.0)
            .await
        {
            Ok(groups) => groups,
            Err(e) => return fetch_error_response(&e),
        };

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

/// Map a [`oidc_groups::FetchGroupsError`] to the REST self-refresh response.
/// A 401 means "re-login" (no session / no token / expired); 5xx is ours.
fn fetch_error_response(e: &oidc_groups::FetchGroupsError) -> Response {
    use oidc_groups::FetchGroupsError as E;
    match e {
        E::NoActiveSession => (StatusCode::UNAUTHORIZED, "no active session").into_response(),
        E::NoStoredToken => (
            StatusCode::UNAUTHORIZED,
            "no stored OIDC tokens on this session; please re-login",
        )
            .into_response(),
        E::TokenExpired => (
            StatusCode::UNAUTHORIZED,
            "OIDC access token expired; please re-login",
        )
            .into_response(),
        E::TokenDecrypt => {
            tracing::error!("decrypting stored OIDC tokens failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "token decryption failed").into_response()
        }
        E::Upstream(m) => {
            tracing::error!(error = %m, "userinfo request failed");
            (StatusCode::BAD_GATEWAY, "userinfo request failed").into_response()
        }
        E::Db(e) => {
            tracing::error!(error = %e, "session lookup failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "session lookup failed").into_response()
        }
    }
}
