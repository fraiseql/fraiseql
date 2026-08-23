//! Lemon Squeezy webhook signature verification.
//!
//! Format: **hex** encoded HMAC-SHA256 (the output of PHP's
//! `hash_hmac('sha256', $payload, $secret)`, which Lemon Squeezy's docs sign
//! with). This verifier compared Base64 until #781 — a 64-char hex digest never
//! equals a 44-char Base64 string, so every genuine delivery was rejected 401.

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use crate::{
    signature::{SignatureError, constant_time_eq},
    traits::SignatureVerifier,
};

/// Verifies Lemon Squeezy webhook signatures using HMAC-SHA256 encoded as hex.
///
/// Lemon Squeezy computes `HMAC-SHA256(secret, body)`, hex-encodes the result, and
/// sends it in the `X-Signature` header.
pub struct LemonSqueezyVerifier;

impl SignatureVerifier for LemonSqueezyVerifier {
    fn name(&self) -> &'static str {
        "lemonsqueezy"
    }

    fn signature_header(&self) -> &'static str {
        "X-Signature"
    }

    fn verify(
        &self,
        payload: &[u8],
        signature: &str,
        secret: &str,
        _timestamp: Option<&str>,
        _url: Option<&str>,
    ) -> Result<bool, SignatureError> {
        if secret.is_empty() {
            return Err(SignatureError::KeyMaterial(
                "Lemon Squeezy signing secret must not be empty".to_string(),
            ));
        }
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::KeyMaterial(e.to_string()))?;
        mac.update(payload);

        let expected = hex::encode(mac.finalize().into_bytes());

        Ok(constant_time_eq(signature.as_bytes(), expected.as_bytes()))
    }
}

#[cfg(test)]
mod tests;
