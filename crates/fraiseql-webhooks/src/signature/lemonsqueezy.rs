//! Lemon Squeezy webhook signature verification.
//!
//! Format: Base64 encoded HMAC-SHA256

use base64::{Engine as _, engine::general_purpose};
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::{
    signature::{SignatureError, constant_time_eq},
    traits::SignatureVerifier,
};

/// Verifies Lemon Squeezy webhook signatures using HMAC-SHA256 encoded as Base64.
///
/// Lemon Squeezy computes `HMAC-SHA256(secret, body)`, Base64-encodes the result, and
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
            return Err(SignatureError::Crypto(
                "Lemon Squeezy signing secret must not be empty".to_string(),
            ));
        }
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::Crypto(e.to_string()))?;
        mac.update(payload);

        let expected = general_purpose::STANDARD.encode(mac.finalize().into_bytes());

        Ok(constant_time_eq(signature.as_bytes(), expected.as_bytes()))
    }
}
