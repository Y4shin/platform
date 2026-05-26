//! `GET /api/me` — the current user as JSON, or 401 when unauthenticated.
//! `POST /api/me/locale` — persist the caller's locale preference.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use junius_sdk::{CurrentUser, Locale, MaybeUser};
use serde::Deserialize;

use crate::auth::AuthState;

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
