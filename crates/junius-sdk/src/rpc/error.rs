//! Connect-RPC error envelope.
//!
//! Connect's error model is HTTP-status + a JSON body shaped like
//! `{"code":"snake_case_status","message":"..."}`. We map our `RpcCode`
//! enum to the canonical HTTP status per the
//! [Connect spec](https://connectrpc.com/docs/protocol/#error-codes).

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// Canonical Connect-RPC status set. Maps 1:1 with gRPC status codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RpcCode {
    Canceled,
    Unknown,
    InvalidArgument,
    DeadlineExceeded,
    NotFound,
    AlreadyExists,
    PermissionDenied,
    ResourceExhausted,
    FailedPrecondition,
    Aborted,
    OutOfRange,
    Unimplemented,
    Internal,
    Unavailable,
    DataLoss,
    Unauthenticated,
}

impl RpcCode {
    /// HTTP status code per the Connect spec.
    pub(crate) fn http_status(self) -> StatusCode {
        match self {
            Self::Canceled => StatusCode::REQUEST_TIMEOUT,
            Self::InvalidArgument | Self::OutOfRange => StatusCode::BAD_REQUEST,
            Self::DeadlineExceeded => StatusCode::GATEWAY_TIMEOUT,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::AlreadyExists | Self::Aborted => StatusCode::CONFLICT,
            Self::PermissionDenied => StatusCode::FORBIDDEN,
            Self::Unauthenticated => StatusCode::UNAUTHORIZED,
            Self::ResourceExhausted => StatusCode::TOO_MANY_REQUESTS,
            Self::FailedPrecondition => StatusCode::PRECONDITION_FAILED,
            Self::Unimplemented => StatusCode::NOT_IMPLEMENTED,
            Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::Unknown | Self::Internal | Self::DataLoss => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// An RPC error. Implements [`IntoResponse`] so handlers can return
/// `Err(RpcError::permission_denied("..."))` directly.
#[derive(Debug, Clone, thiserror::Error)]
#[error("[{code:?}] {message}")]
pub struct RpcError {
    pub code: RpcCode,
    pub message: String,
}

impl RpcError {
    #[must_use]
    pub fn new(code: RpcCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self::new(RpcCode::InvalidArgument, message)
    }

    #[must_use]
    pub fn unimplemented(message: impl Into<String>) -> Self {
        Self::new(RpcCode::Unimplemented, message)
    }

    #[must_use]
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(RpcCode::Internal, message)
    }

    #[must_use]
    pub fn permission_denied(message: impl Into<String>) -> Self {
        Self::new(RpcCode::PermissionDenied, message)
    }

    #[must_use]
    pub fn unauthenticated(message: impl Into<String>) -> Self {
        Self::new(RpcCode::Unauthenticated, message)
    }

    #[must_use]
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(RpcCode::NotFound, message)
    }
}

#[derive(Serialize)]
struct WireError<'a> {
    code: RpcCode,
    message: &'a str,
}

impl IntoResponse for RpcError {
    fn into_response(self) -> Response {
        let body = serde_json::to_vec(&WireError {
            code: self.code,
            message: &self.message,
        })
        .unwrap_or_else(|_| br#"{"code":"internal","message":"failed to encode error"}"#.to_vec());

        let mut response = (self.code.http_status(), body).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        response
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use axum::body::to_bytes;
    use http::StatusCode;

    #[tokio::test]
    async fn permission_denied_maps_to_403() {
        let resp = RpcError::permission_denied("nope").into_response();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        let bytes = to_bytes(resp.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["code"], "permission_denied");
        assert_eq!(v["message"], "nope");
    }

    #[tokio::test]
    async fn invalid_argument_maps_to_400() {
        let resp = RpcError::invalid_argument("bad").into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn internal_maps_to_500() {
        let resp = RpcError::internal("boom").into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
