//! Session middleware: resolve the `session` cookie to a `User` and attach it to
//! request extensions. It never rejects — route-level extractors (`CurrentUser`)
//! enforce authentication; permission enforcement arrives in M07.

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use tower_cookies::Cookies;
use uuid::Uuid;

use crate::auth::AuthState;
use crate::auth::user_query::load_user_by_session;

/// Name of the session cookie (`HttpOnly`; the value is the session id).
pub const SESSION_COOKIE: &str = "session";

pub async fn middleware(State(state): State<AuthState>, mut req: Request, next: Next) -> Response {
    if let Some(cookies) = req.extensions().get::<Cookies>().cloned() {
        if let Some(cookie) = cookies.get(SESSION_COOKIE) {
            if let Ok(id) = Uuid::parse_str(cookie.value()) {
                match load_user_by_session(&state.pool, id).await {
                    Ok(Some(user)) => {
                        req.extensions_mut().insert(user);
                    }
                    Ok(None) => {}
                    Err(e) => tracing::warn!(error = %e, "session lookup failed"),
                }
            }
        }
    }
    next.run(req).await
}
