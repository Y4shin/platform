//! Connect-RPC unary service builder.
//!
//! Each plugin's `Plugin::rpc_routes()` constructs one (or more) services
//! via [`ServiceBuilder`]. The host nests the resulting router under `/rpc`.
//! A Connect-Web client calls `<baseUrl>/<typeName>/<method>` and the
//! router's path layout matches.

use std::future::Future;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Router, http::StatusCode};

use super::codec::{self, Codec};
use super::error::RpcError;

/// Result alias for RPC handlers.
pub type RpcResult<T> = Result<T, RpcError>;

/// Fluent builder that compiles a Connect-RPC service into an [`axum::Router`].
///
/// ```ignore
/// let router = ServiceBuilder::new("hello.v1.HelloService")
///     .unary("Greet", greet_handler)
///     .into_router();
/// ```
pub struct ServiceBuilder {
    type_name: &'static str,
    router: Router,
}

impl ServiceBuilder {
    /// Start a new service. `type_name` is the proto package + service name
    /// joined with a dot — exactly what the Connect-Web client expects in the
    /// URL (e.g. `"hello.v1.HelloService"`).
    #[must_use]
    pub fn new(type_name: &'static str) -> Self {
        Self {
            type_name,
            router: Router::new(),
        }
    }

    /// Register a unary RPC method.
    ///
    /// The handler's signature is `async fn(Req) -> RpcResult<Res>`. Both
    /// types must be a `prost::Message` (for binary) and serde-serialisable
    /// (for JSON).
    #[must_use]
    pub fn unary<Req, Res, F, Fut>(mut self, method_name: &'static str, handler: F) -> Self
    where
        Req: prost::Message + serde::de::DeserializeOwned + Default + Send + 'static,
        Res: prost::Message + serde::Serialize + Send + 'static,
        F: Fn(Req) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future<Output = RpcResult<Res>> + Send + 'static,
    {
        let path = format!("/{}/{method_name}", self.type_name);
        let dispatcher = Dispatcher::new(handler);
        self.router = self.router.route(
            &path,
            post(dispatch::<Req, Res, F, Fut>).with_state(Arc::new(dispatcher)),
        );
        self
    }

    /// Finish the builder; the returned router is unprefixed (its only
    /// routes are `/<type_name>/<method>` entries).
    pub fn into_router(self) -> Router {
        self.router
    }
}

/// Type-erased holder for a handler, so each registered method can share
/// the generic `dispatch` function below via its own `State`.
struct Dispatcher<F> {
    handler: F,
}

impl<F> Dispatcher<F> {
    fn new(handler: F) -> Self {
        Self { handler }
    }
}

async fn dispatch<Req, Res, F, Fut>(
    State(dispatcher): State<Arc<Dispatcher<F>>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response
where
    Req: prost::Message + serde::de::DeserializeOwned + Default + Send + 'static,
    Res: prost::Message + serde::Serialize + Send + 'static,
    F: Fn(Req) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = RpcResult<Res>> + Send + 'static,
{
    let codec = match Codec::from_headers(&headers) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };

    let request: Req = match codec::decode_request(codec, &body) {
        Ok(r) => r,
        Err(e) => return e.into_response(),
    };

    let result = (dispatcher.handler)(request).await;
    let response = match result {
        Ok(r) => r,
        Err(e) => return e.into_response(),
    };

    let body = match codec::encode_response(codec, &response) {
        Ok(b) => b,
        Err(e) => return e.into_response(),
    };

    let mut response = (StatusCode::OK, body).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, codec.response_content_type());
    response
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::{HeaderValue, Request, StatusCode, header};
    use prost::Message;
    use tower::ServiceExt;

    /// Hand-rolled `prost::Message` (avoids pulling prost-build into junius-sdk's tests).
    #[derive(Clone, PartialEq, prost::Message, serde::Serialize, serde::Deserialize)]
    struct EchoRequest {
        #[prost(string, tag = "1")]
        msg: String,
    }

    #[derive(Clone, PartialEq, prost::Message, serde::Serialize, serde::Deserialize)]
    struct EchoResponse {
        #[prost(string, tag = "1")]
        msg: String,
    }

    fn echo_router() -> Router {
        ServiceBuilder::new("test.v1.EchoService")
            .unary("Echo", |req: EchoRequest| async move {
                Ok(EchoResponse { msg: req.msg })
            })
            .unary("Deny", |_: EchoRequest| async move {
                Err::<EchoResponse, _>(RpcError::permission_denied("no"))
            })
            .into_router()
    }

    #[tokio::test]
    async fn binary_roundtrip() {
        let app = echo_router();
        let body = EchoRequest {
            msg: "hello".to_string(),
        }
        .encode_to_vec();

        let response = app
            .oneshot(
                Request::post("/test.v1.EchoService/Echo")
                    .header(header::CONTENT_TYPE, "application/proto")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE),
            Some(&HeaderValue::from_static("application/proto"))
        );

        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        let decoded = EchoResponse::decode(&bytes[..]).unwrap();
        assert_eq!(decoded.msg, "hello");
    }

    #[tokio::test]
    async fn json_roundtrip() {
        let app = echo_router();
        let response = app
            .oneshot(
                Request::post("/test.v1.EchoService/Echo")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"msg":"alice"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE),
            Some(&HeaderValue::from_static("application/json"))
        );

        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["msg"], "alice");
    }

    #[tokio::test]
    async fn handler_error_maps_to_status_and_json_envelope() {
        let app = echo_router();
        let response = app
            .oneshot(
                Request::post("/test.v1.EchoService/Deny")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"msg":"x"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["code"], "permission_denied");
        assert_eq!(v["message"], "no");
    }

    #[tokio::test]
    async fn unknown_method_404s() {
        let app = echo_router();
        let response = app
            .oneshot(
                Request::post("/test.v1.EchoService/Missing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn malformed_body_returns_invalid_argument() {
        let app = echo_router();
        // JSON body but binary content type → prost decode fails.
        let response = app
            .oneshot(
                Request::post("/test.v1.EchoService/Echo")
                    .header(header::CONTENT_TYPE, "application/proto")
                    .body(Body::from(r#"{"msg":"x"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn missing_content_type_defaults_to_proto() {
        let app = echo_router();
        let body = EchoRequest {
            msg: "x".to_string(),
        }
        .encode_to_vec();

        let response = app
            .oneshot(
                Request::post("/test.v1.EchoService/Echo")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
