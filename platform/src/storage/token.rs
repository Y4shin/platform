//! Short-lived, op/object-scoped signed tokens for the juniusd-mediated storage
//! endpoints (M10 Stage 7). HMAC-SHA256 over a compact JSON payload, base64url
//! encoded as `payload.signature`. Verification is constant-time (via the MAC)
//! and checks expiry. The secret is `[config.storage].token_secret`.

use base64::Engine as _;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;
const B64: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// What a token authorizes: a single op on a single object.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StorageToken {
    /// `'p'` = upload (PUT), `'g'` = download (GET).
    pub op: char,
    pub bucket: String,
    pub key: String,
    /// Unix-seconds expiry.
    pub exp: i64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TokenError {
    #[error("malformed token")]
    Malformed,
    #[error("bad signature")]
    BadSignature,
    #[error("token expired")]
    Expired,
}

/// Signs + verifies [`StorageToken`]s with a shared secret.
#[derive(Clone)]
pub struct TokenSigner {
    secret: Vec<u8>,
}

impl TokenSigner {
    #[must_use]
    pub fn new(secret: &str) -> Self {
        Self {
            secret: secret.as_bytes().to_vec(),
        }
    }

    #[allow(
        clippy::unwrap_used,
        reason = "HMAC-SHA256 accepts any key length; small-struct JSON cannot fail"
    )]
    pub fn sign(&self, op: char, bucket: &str, key: &str, ttl_secs: u32) -> String {
        let token = StorageToken {
            op,
            bucket: bucket.to_string(),
            key: key.to_string(),
            exp: chrono::Utc::now().timestamp() + i64::from(ttl_secs),
        };
        let payload = serde_json::to_vec(&token).unwrap();
        let mut mac = HmacSha256::new_from_slice(&self.secret).unwrap();
        mac.update(&payload);
        let sig = mac.finalize().into_bytes();
        format!("{}.{}", B64.encode(&payload), B64.encode(sig))
    }

    pub fn verify(&self, token: &str) -> Result<StorageToken, TokenError> {
        let (payload_b64, sig_b64) = token.split_once('.').ok_or(TokenError::Malformed)?;
        let payload = B64.decode(payload_b64).map_err(|_| TokenError::Malformed)?;
        let sig = B64.decode(sig_b64).map_err(|_| TokenError::Malformed)?;
        let mut mac =
            HmacSha256::new_from_slice(&self.secret).map_err(|_| TokenError::Malformed)?;
        mac.update(&payload);
        mac.verify_slice(&sig)
            .map_err(|_| TokenError::BadSignature)?;
        let parsed: StorageToken =
            serde_json::from_slice(&payload).map_err(|_| TokenError::Malformed)?;
        if parsed.exp < chrono::Utc::now().timestamp() {
            return Err(TokenError::Expired);
        }
        Ok(parsed)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests panic on unexpected failures")]
mod tests {
    use super::*;

    #[test]
    fn round_trip_valid() {
        let s = TokenSigner::new("secret");
        let t = s.sign('p', "main", "hello/x.txt", 60);
        let v = s.verify(&t).unwrap();
        assert_eq!(v.op, 'p');
        assert_eq!(v.bucket, "main");
        assert_eq!(v.key, "hello/x.txt");
    }

    #[test]
    fn rejects_tampered_signature() {
        let s = TokenSigner::new("secret");
        let t = s.sign('g', "main", "hello/x.txt", 60);
        let mut bad = t.clone();
        bad.pop();
        bad.push(if t.ends_with('A') { 'B' } else { 'A' });
        assert!(matches!(
            s.verify(&bad),
            Err(TokenError::BadSignature | TokenError::Malformed)
        ));
    }

    #[test]
    fn rejects_wrong_secret() {
        let t = TokenSigner::new("secret-a").sign('g', "main", "k", 60);
        assert_eq!(
            TokenSigner::new("secret-b").verify(&t),
            Err(TokenError::BadSignature)
        );
    }

    #[test]
    fn rejects_expired() {
        let s = TokenSigner::new("secret");
        let t = s.sign('g', "main", "k", 0);
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert_eq!(s.verify(&t), Err(TokenError::Expired));
    }

    #[test]
    fn rejects_malformed() {
        let s = TokenSigner::new("secret");
        assert_eq!(s.verify("not-a-token"), Err(TokenError::Malformed));
    }
}
