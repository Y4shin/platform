//! Unauthenticated `.ics` HTTP endpoints, mounted under the plugin's
//! `http_prefix` (`/h/events`). These are the calendar-export surface for
//! external apps that poll without a session:
//!
//! - `GET /ics/e/{id}`        — one event, by the **event ACL** (public open;
//!   private needs a session caller with read access, else 404).
//! - `GET /ics/u/{key}`       — a personal feed, by the **key** (resolved against
//!   the key owner's live entitlements).
//! - `GET /ics/g/{name}`      — a group feed, open **only if** the group opted its
//!   calendar public.
//! - `GET /ics/g/{name}/{key}`— a group feed, by the **key** (works while private;
//!   the key's group is authoritative, so `{name}` is cosmetic).
//!
//! All handlers run **caller-lessly** on the plugin pool (the M10 `system_context`
//! style): they build a `CalendarRepo<()>` and resolve against the subject, never
//! the anonymous caller. A revoked/absent key or a forbidden event → 404.

use axum::{
    Router,
    body::Body,
    extract::Path,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use junius_sdk::PluginResources;
use uuid::Uuid;

use crate::domain::{FeedKind, hash_token};
use crate::ics::{IcsEvent, render_calendar};
use crate::repo::CalendarRepo;

/// The `.ics` sub-router (mounted under `/h/events`).
pub fn routes() -> Router {
    Router::new()
        .route("/ics/e/{id}", get(event_ics))
        .route("/ics/u/{key}", get(personal_feed))
        .route("/ics/g/{name}", get(group_public_feed))
        .route("/ics/g/{name}/{key}", get(group_keyed_feed))
}

/// A caller-less calendar repo over the plugin pool.
fn calendar(resources: &PluginResources) -> CalendarRepo<()> {
    CalendarRepo::new(resources.db(), None, resources.audit.clone())
}

/// `GET /ics/e/{id}` — single event, by the event ACL (uses the session caller if
/// present, so a private event's owner can export it while logged in).
async fn event_ics(resources: PluginResources, Path(id): Path<String>) -> Response {
    let Ok(event_id) = Uuid::parse_str(&id) else {
        return not_found();
    };
    let viewer = resources.auth.current_user().map(|u| u.id.0);
    match calendar(&resources).event_for_ics(event_id, viewer).await {
        Ok(Some(event)) => {
            let name = event.title.clone();
            ics_response(&name, &[event])
        }
        Ok(None) => not_found(),
        Err(e) => internal(&e),
    }
}

/// `GET /ics/u/{key}` — personal feed, by key.
async fn personal_feed(resources: PluginResources, Path(key): Path<String>) -> Response {
    let repo = calendar(&resources);
    let token = match repo.lookup_token(&hash_token(&key)).await {
        Ok(Some(t)) if t.kind == FeedKind::Personal => t,
        Ok(_) => return not_found(),
        Err(e) => return internal(&e),
    };
    let Some(subject) = token.subject_user_id else {
        return not_found();
    };
    serve_feed(repo.personal_feed(subject).await, "Personal calendar")
}

/// `GET /ics/g/{name}` — group feed, open only if the group opted public.
async fn group_public_feed(resources: PluginResources, Path(name): Path<String>) -> Response {
    let Some(group) = (match resources.groups.by_name(&name).await {
        Ok(g) => g,
        Err(e) => return internal(&e),
    }) else {
        return not_found();
    };
    match calendar(&resources).group_is_public(group.id.0).await {
        Ok(true) => {}
        Ok(false) => return not_found(),
        Err(e) => return internal(&e),
    }
    serve_feed(
        calendar(&resources).group_feed(group.id.0).await,
        &group.name,
    )
}

/// `GET /ics/g/{name}/{key}` — group feed, by key (the key's group is
/// authoritative; `{name}` is cosmetic).
async fn group_keyed_feed(
    resources: PluginResources,
    Path((_name, key)): Path<(String, String)>,
) -> Response {
    let repo = calendar(&resources);
    let token = match repo.lookup_token(&hash_token(&key)).await {
        Ok(Some(t)) if t.kind == FeedKind::Group => t,
        Ok(_) => return not_found(),
        Err(e) => return internal(&e),
    };
    let Some(group_id) = token.subject_group_id else {
        return not_found();
    };
    let label = resources
        .groups
        .by_id(junius_sdk::GroupId(group_id))
        .await
        .ok()
        .flatten()
        .map_or_else(|| "Group calendar".to_string(), |g| g.name);
    serve_feed(repo.group_feed(group_id).await, &label)
}

/// Render a feed query result, or 404/500 on miss/error.
fn serve_feed(events: Result<Vec<IcsEvent>, junius_sdk::RepoError>, name: &str) -> Response {
    match events {
        Ok(events) => ics_response(name, &events),
        Err(e) => internal(&e),
    }
}

/// A `text/calendar` 200 response.
fn ics_response(name: &str, events: &[IcsEvent]) -> Response {
    let body = render_calendar(name, events);
    let mut response = Response::new(Body::from(body));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/calendar; charset=utf-8"),
    );
    response
}

fn not_found() -> Response {
    StatusCode::NOT_FOUND.into_response()
}

fn internal(e: impl std::fmt::Display) -> Response {
    tracing::error!(error = %e, "ics endpoint failed");
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}
