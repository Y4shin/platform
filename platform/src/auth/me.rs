//! `GET /api/me` — the current user as JSON, or 401 when unauthenticated.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use junius_sdk::MaybeUser;

pub async fn handler(MaybeUser(user): MaybeUser) -> Response {
    match user {
        Some(user) => Json(user).into_response(),
        None => (StatusCode::UNAUTHORIZED, "not authenticated").into_response(),
    }
}
