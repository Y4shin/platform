//! `DELETE /api/sessions/<id>` — revoke one of the caller's own sessions.
//!
//! Backs the Sessions table on the `/me` profile page. The authorization gates:
//! - the caller must be authenticated ([`CurrentUser`] → 401 otherwise);
//! - the request's **current** session is protected from revoke (403) to avoid
//!   accidental self-lockout — the current session signs out via the user menu;
//! - a session that isn't the caller's own (another user's, or nonexistent) is a
//!   404, scoped by `user_id` so we never disclose the existence of other users'
//!   sessions.
//!
//! Deleting the row is what forces re-login: the next request bearing that
//! cookie resolves no session ([`session::middleware`](super::session)), so the
//! browser is bounced to login.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use junius_sdk::CurrentUser;
use tower_cookies::Cookies;
use uuid::Uuid;

use crate::auth::AuthState;
use crate::auth::session::SESSION_COOKIE;

pub async fn revoke_session(
    State(state): State<AuthState>,
    cookies: Cookies,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Response {
    let current_session = cookies
        .get(SESSION_COOKIE)
        .and_then(|c| Uuid::parse_str(c.value()).ok());
    if current_session == Some(id) {
        return (
            StatusCode::FORBIDDEN,
            "cannot revoke the current session; sign out instead",
        )
            .into_response();
    }

    // Scope the delete to the caller's own sessions: a row that isn't theirs
    // (another user's, or a bogus id) affects zero rows → 404, leaking nothing.
    match sqlx::query("DELETE FROM platform.session WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user.id.0)
        .execute(&state.pool)
        .await
    {
        Ok(result) if result.rows_affected() == 1 => StatusCode::NO_CONTENT.into_response(),
        Ok(_) => (StatusCode::NOT_FOUND, "session not found").into_response(),
        Err(e) => {
            tracing::error!(error = %e, "failed to revoke session");
            (StatusCode::INTERNAL_SERVER_ERROR, "failed").into_response()
        }
    }
}
