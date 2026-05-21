//! SPA static-asset serving. Only compiled when the `embed-frontend` feature
//! is on; `junius build` flips it on in release builds. In dev mode Vite
//! serves the FE directly and this module is absent.

use axum::{
    Router,
    body::Body,
    extract::Path,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/frontend/dist"]
struct Assets;

/// Build the static-asset sub-router. Routes:
/// - `GET /assets/*` → hashed asset (returns 404 if absent).
/// - `GET /*` → SPA fallback. Any unmatched path returns `index.html` so
///   client-side routing handles it.
pub fn router() -> Router {
    Router::new()
        .route("/assets/*path", get(serve_asset))
        .fallback(get(spa_fallback))
}

async fn serve_asset(Path(path): Path<String>) -> Response {
    let key = format!("assets/{path}");
    match Assets::get(&key) {
        Some(content) => render_asset(&key, content),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn spa_fallback() -> Response {
    match Assets::get("index.html") {
        Some(content) => render_asset("index.html", content),
        None => (StatusCode::NOT_FOUND, "index.html not embedded").into_response(),
    }
}

fn render_asset(key: &str, content: rust_embed::EmbeddedFile) -> Response {
    let mime = mime_guess::from_path(key).first_or_octet_stream();
    let mut response = Response::new(Body::from(content.data));
    if let Ok(value) = HeaderValue::from_str(mime.as_ref()) {
        response.headers_mut().insert(header::CONTENT_TYPE, value);
    }
    response
}
