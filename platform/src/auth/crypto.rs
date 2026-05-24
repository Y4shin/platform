//! Symmetric encryption for OIDC tokens stored in `platform.session.oidc_tokens`
//! and the key used for private (encrypted) flow cookies. Both are derived from
//! the deployment's `session_encryption_key`.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::RngCore;
use sha2::{Digest, Sha256, Sha512};

/// AES-256-GCM cipher derived from the configured key material (SHA-256 → 32 B).
#[must_use]
pub fn cipher_from_key(key_material: &str) -> Aes256Gcm {
    let digest = Sha256::digest(key_material.as_bytes());
    Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&digest))
}

/// 64-byte key material for `tower_cookies::Key` (private cookies), derived from
/// the same secret via SHA-512.
#[must_use]
pub fn cookie_key_material(key_material: &str) -> [u8; 64] {
    Sha512::digest(key_material.as_bytes()).into()
}

/// Encrypt `plaintext`, returning `nonce(12) || ciphertext`.
#[must_use]
pub fn encrypt(cipher: &Aes256Gcm, plaintext: &[u8]) -> Vec<u8> {
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    // AES-GCM encryption only fails on absurd input sizes we never hit here.
    let mut out = nonce_bytes.to_vec();
    match cipher.encrypt(nonce, plaintext) {
        Ok(mut ct) => {
            out.append(&mut ct);
            out
        }
        Err(_) => out, // degrade to just the nonce; decrypt will fail closed
    }
}

/// Decrypt data produced by [`encrypt`]. Returns `None` on any failure.
#[must_use]
pub fn decrypt(cipher: &Aes256Gcm, data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 12 {
        return None;
    }
    let (nonce_bytes, ct) = data.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher.decrypt(nonce, ct).ok()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests panic on unexpected failures")]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let cipher = cipher_from_key("a dev secret");
        let secret = b"oidc-access-token";
        let blob = encrypt(&cipher, secret);
        assert_ne!(&blob[12..], secret, "ciphertext differs from plaintext");
        assert_eq!(decrypt(&cipher, &blob).unwrap(), secret);
    }

    #[test]
    fn wrong_key_fails_closed() {
        let blob = encrypt(&cipher_from_key("key-a"), b"data");
        assert!(decrypt(&cipher_from_key("key-b"), &blob).is_none());
    }

    #[test]
    fn truncated_input_is_none() {
        assert!(decrypt(&cipher_from_key("k"), &[0u8; 4]).is_none());
    }
}
