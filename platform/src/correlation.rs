//! Request-correlation middleware (M19 slice #5).
//!
//! Every request is wrapped in a `tracing` span; when the `OTel` trace layer is
//! installed (`[config.otel] enabled = true`) that span carries a valid trace
//! id. This middleware surfaces that id two ways so a user-facing error and the
//! `juniusd` logs can be cross-referenced:
//!
//! - it stamps the trace id onto the response as the `x-correlation-id` header,
//!   which Connect-Web exposes to the SPA as `ConnectError.metadata`; the
//!   frontend's `<RouteErrorBoundary>` / `<ForbiddenPage>` shows it to the user.
//! - on a 5xx it emits a `tracing::error!` carrying the same `correlation_id`,
//!   so the operator can grep the logs for the id the user reports.
//!
//! When `OTel` is disabled there is no valid trace id; the header is simply
//! omitted (the frontend then falls back to generic copy — no white screen).

use axum::extract::Request;
use axum::http::{HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;
use opentelemetry::trace::TraceContextExt as _;
use tracing::Instrument as _;
use tracing_opentelemetry::OpenTelemetrySpanExt as _;

/// The response header carrying the current trace id to the SPA.
pub const CORRELATION_HEADER: HeaderName = HeaderName::from_static("x-correlation-id");

/// Read the current span's `OTel` trace id, if one is active and valid. Returns
/// `None` when no `OTel` trace layer is installed (telemetry disabled) — the
/// span context is then the invalid all-zero default.
fn current_trace_id() -> Option<String> {
    let cx = tracing::Span::current().context();
    let span = cx.span();
    let sc = span.span_context();
    sc.is_valid().then(|| sc.trace_id().to_string())
}

/// Wrap each request in a span and, when a valid trace id is present, stamp it
/// onto the response as `x-correlation-id` (read by the SPA via
/// `ConnectError.metadata`). On a 5xx, also emit a `tracing::error!` carrying
/// the same `correlation_id` so the user-reported id is greppable in the logs.
pub async fn stamp(req: Request, next: Next) -> Response {
    let span = tracing::info_span!(
        "http_request",
        method = %req.method(),
        path = %req.uri().path(),
    );
    async move {
        let trace_id = current_trace_id();
        let mut response = next.run(req).await;
        if let Some(id) = trace_id {
            if response.status().is_server_error() {
                tracing::error!(
                    correlation_id = %id,
                    status = response.status().as_u16(),
                    "request failed",
                );
            }
            if let Ok(value) = HeaderValue::from_str(&id) {
                response.headers_mut().insert(CORRELATION_HEADER, value);
            }
        }
        response
    }
    .instrument(span)
    .await
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests panic on failure"
)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request as HttpRequest, StatusCode};
    use axum::routing::get;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use tower::ServiceExt as _;
    use tracing::field::{Field, Visit};
    use tracing_subscriber::Registry;
    use tracing_subscriber::layer::{Context, Layer, SubscriberExt as _};

    use super::*;

    /// A tracing layer that records the `correlation_id` field of every event
    /// into a shared vec, so a test can assert the logged id matches the header.
    #[derive(Clone, Default)]
    struct CaptureLayer {
        ids: Arc<Mutex<Vec<String>>>,
    }

    struct CorrelationVisitor<'a>(&'a mut Option<String>);
    impl Visit for CorrelationVisitor<'_> {
        fn record_str(&mut self, field: &Field, value: &str) {
            if field.name() == "correlation_id" {
                *self.0 = Some(value.to_string());
            }
        }
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            if field.name() == "correlation_id" {
                *self.0 = Some(format!("{value:?}").trim_matches('"').to_string());
            }
        }
    }

    impl<S: tracing::Subscriber> Layer<S> for CaptureLayer {
        fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
            let mut found = None;
            event.record(&mut CorrelationVisitor(&mut found));
            if let Some(id) = found {
                self.ids.lock().unwrap().push(id);
            }
        }
    }

    fn app() -> Router {
        Router::new()
            .route("/boom", get(|| async { StatusCode::INTERNAL_SERVER_ERROR }))
            .route("/ok", get(|| async { StatusCode::OK }))
            .layer(axum::middleware::from_fn(stamp))
    }

    async fn get_response(path: &str) -> Response {
        app()
            .oneshot(
                HttpRequest::builder()
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    #[test]
    fn error_response_carries_trace_id_matching_the_log() {
        let capture = CaptureLayer::default();
        // A real OTel tracer (no exporter) so spans get valid, sampled trace ids
        // without any network.
        let provider = SdkTracerProvider::builder().build();
        let otel = tracing_opentelemetry::layer().with_tracer(
            opentelemetry::trace::TracerProvider::tracer(&provider, "test"),
        );
        let subscriber = Registry::default().with(otel).with(capture.clone());

        let header = tracing::subscriber::with_default(subscriber, || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                let resp = get_response("/boom").await;
                assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
                resp.headers()
                    .get(CORRELATION_HEADER)
                    .map(|v| v.to_str().unwrap().to_string())
            })
        });

        let header = header.expect("error response must carry x-correlation-id when OTel is on");
        assert_eq!(header.len(), 32, "trace id is 32 lowercase hex chars");
        assert!(header.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(
            header,
            "0".repeat(32),
            "trace id must be non-zero (a real span)"
        );

        // The same id the user sees in the header must appear in the logs.
        let logged = capture.ids.lock().unwrap().clone();
        assert!(
            logged.contains(&header),
            "logged correlation_id {logged:?} must include the header id {header}"
        );
    }

    #[test]
    fn no_trace_id_header_without_an_otel_layer() {
        // fmt/registry only, no OTel layer → no valid trace id → no header
        // (graceful degradation when telemetry export is disabled).
        let subscriber = Registry::default();
        let header_present = tracing::subscriber::with_default(subscriber, || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                get_response("/boom")
                    .await
                    .headers()
                    .contains_key(CORRELATION_HEADER)
            })
        });
        assert!(
            !header_present,
            "no correlation header when OTel is disabled"
        );
    }
}
