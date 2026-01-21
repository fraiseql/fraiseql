//! Shopify webhook signature verification.
//!
//! Format: Base64 encoded HMAC-SHA256

use crate::signature::{constant_time_eq, SignatureError};
use crate::traits::SignatureVerifier;
use base64::{engine::general_purpose, Engine as _};
use hmac::{Hmac, Mac};
use sha2::Sha256;

pub struct ShopifyVerifier;

impl SignatureVerifier for ShopifyVerifier {
    fn name(&self) -> &'static str {
        "shopify"
    }

    fn signature_header(&self) -> &'static str {
        "X-Shopify-Hmac-Sha256"
    }

    fn verify(
        &self,
        payload: &[u8],
        signature: &str,
        secret: &str,
        _timestamp: Option<&str>,
    ) -> Result<bool, SignatureError> {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::Crypto(e.to_string()))?;
        mac.update(payload);

        let expected = general_purpose::STANDARD.encode(mac.finalize().into_bytes());

        Ok(constant_time_eq(signature.as_bytes(), expected.as_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_signature(payload: &[u8], secret: &str) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload);
        general_purpose::STANDARD.encode(mac.finalize().into_bytes())
    }

    #[test]
    fn test_valid_signature() {
        let verifier = ShopifyVerifier;
        let payload = b"test payload";
        let secret = "secret";
        let signature = generate_signature(payload, secret);

        assert!(verifier.verify(payload, &signature, secret, None).unwrap());
    }

    #[test]
    fn test_invalid_signature() {
        let verifier = ShopifyVerifier;
        assert!(!verifier
            .verify(b"test", "invalid", "secret", None)
            .unwrap());
    }
}
