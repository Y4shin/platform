//! Wire-format codecs for Connect unary requests/responses.
//!
//! Connect supports `application/proto` (binary protobuf, the default) and
//! `application/json` (proto3-JSON). We implement both via prost (for binary)
//! and plain serde (for JSON). The serde path matches Connect-Web's JSON
//! output for scalar-only messages used in M05; proto3-JSON spec quirks
//! (uint64-as-string, well-known types) become relevant later and can be
//! addressed via prost-wkt or per-field attribute overrides.

use axum::http::{HeaderMap, HeaderValue, header};

use super::error::{RpcCode, RpcError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Codec {
    Proto,
    Json,
}

impl Codec {
    /// The `Content-Type` value to use when emitting a response in this codec.
    pub(crate) fn response_content_type(self) -> HeaderValue {
        match self {
            Self::Proto => HeaderValue::from_static("application/proto"),
            Self::Json => HeaderValue::from_static("application/json"),
        }
    }

    /// Pick a codec from the request's `Content-Type` header.
    ///
    /// Defaults to `Proto` when the header is missing (Connect's spec default).
    /// Returns `Err(InvalidArgument)` for any other unrecognised content type
    /// so plugin authors don't silently fall back to binary for typos.
    pub(crate) fn from_headers(headers: &HeaderMap) -> Result<Self, RpcError> {
        let Some(value) = headers.get(header::CONTENT_TYPE) else {
            return Ok(Self::Proto);
        };
        let raw = value
            .to_str()
            .map_err(|_| RpcError::invalid_argument("Content-Type is not valid ASCII"))?;
        // Trim any "; charset=..." suffix.
        let mime = raw.split(';').next().unwrap_or(raw).trim();
        match mime {
            "application/proto" | "application/x-protobuf" => Ok(Self::Proto),
            "application/json" => Ok(Self::Json),
            other => Err(RpcError::new(
                RpcCode::InvalidArgument,
                format!("unsupported Content-Type: {other:?}"),
            )),
        }
    }
}

pub(crate) fn decode_request<T>(codec: Codec, body: &[u8]) -> Result<T, RpcError>
where
    T: prost::Message + serde::de::DeserializeOwned + Default,
{
    match codec {
        Codec::Proto => T::decode(body).map_err(|e| {
            RpcError::invalid_argument(format!("malformed protobuf request body: {e}"))
        }),
        Codec::Json => serde_json::from_slice::<T>(body)
            .map_err(|e| RpcError::invalid_argument(format!("malformed JSON request body: {e}"))),
    }
}

pub(crate) fn encode_response<T>(codec: Codec, value: &T) -> Result<Vec<u8>, RpcError>
where
    T: prost::Message + serde::Serialize,
{
    match codec {
        Codec::Proto => Ok(value.encode_to_vec()),
        Codec::Json => serde_json::to_vec(value)
            .map_err(|e| RpcError::internal(format!("failed to encode JSON response: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use http::HeaderMap;

    fn headers_with_ct(ct: &'static str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(header::CONTENT_TYPE, HeaderValue::from_static(ct));
        h
    }

    #[test]
    fn missing_content_type_defaults_to_proto() {
        let codec = Codec::from_headers(&HeaderMap::new()).unwrap();
        assert_eq!(codec, Codec::Proto);
    }

    #[test]
    fn application_proto_parses() {
        assert_eq!(
            Codec::from_headers(&headers_with_ct("application/proto")).unwrap(),
            Codec::Proto
        );
    }

    #[test]
    fn x_protobuf_parses_as_proto() {
        assert_eq!(
            Codec::from_headers(&headers_with_ct("application/x-protobuf")).unwrap(),
            Codec::Proto
        );
    }

    #[test]
    fn application_json_parses() {
        assert_eq!(
            Codec::from_headers(&headers_with_ct("application/json")).unwrap(),
            Codec::Json
        );
    }

    #[test]
    fn json_with_charset_suffix_parses() {
        assert_eq!(
            Codec::from_headers(&headers_with_ct("application/json; charset=utf-8")).unwrap(),
            Codec::Json
        );
    }

    #[test]
    fn unknown_content_type_rejected() {
        let err = Codec::from_headers(&headers_with_ct("text/html")).unwrap_err();
        assert_eq!(err.code, RpcCode::InvalidArgument);
    }
}
