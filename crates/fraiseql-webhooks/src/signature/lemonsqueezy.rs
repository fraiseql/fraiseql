//! Lemon Squeezy webhook signature verification.
//!
//! Format: **hex** encoded HMAC-SHA256 (the output of PHP's
//! `hash_hmac('sha256', $payload, $secret)`, which Lemon Squeezy's docs sign
//! with). This verifier compared Base64 until #781 — a 64-char hex digest never
//! equals a 44-char Base64 string, so every genuine delivery was rejected 401.

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use crate::{
    request::InboundRequest,
    signature::{SignatureError, Verified, constant_time_eq, verified_if},
    traits::SignatureVerifier,
};

/// Verifies Lemon Squeezy webhook signatures using HMAC-SHA256 encoded as hex.
///
/// Lemon Squeezy computes `HMAC-SHA256(secret, body)`, hex-encodes the result, and
/// sends it in the `X-Signature` header.
pub struct LemonSqueezyVerifier;

/// The header this scheme reads its credential from.
const SIGNATURE_HEADER: &str = "X-Signature";

impl SignatureVerifier for LemonSqueezyVerifier {
    fn name(&self) -> &'static str {
        "lemonsqueezy"
    }

    fn verify(
        &self,
        request: &InboundRequest<'_>,
        secret: &str,
    ) -> Result<Verified, SignatureError> {
        let signature = request.require_header(SIGNATURE_HEADER)?;
        let payload = request.body();
        if secret.is_empty() {
            return Err(SignatureError::KeyMaterial(
                "Lemon Squeezy signing secret must not be empty".to_string(),
            ));
        }
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::KeyMaterial(e.to_string()))?;
        mac.update(payload);

        let expected = hex::encode(mac.finalize().into_bytes());

        verified_if(constant_time_eq(signature.as_bytes(), expected.as_bytes()))
    }
}

#[cfg(test)]
mod tests;
