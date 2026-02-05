//! GitHub webhook signature verification.
//!
//! Format: `sha256=<hex>`
//! Algorithm: HMAC-SHA256

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::webhooks::{
    signature::{SignatureError, constant_time_eq},
    traits::SignatureVerifier,
};

pub struct GitHubVerifier;

impl SignatureVerifier for GitHubVerifier {
    fn name(&self) -> &'static str {
        "github"
    }

    fn signature_header(&self) -> &'static str {
        "X-Hub-Signature-256"
    }

    fn verify(
        &self,
        payload: &[u8],
        signature: &str,
        secret: &str,
        _timestamp: Option<&str>,
    ) -> Result<bool, SignatureError> {
        // GitHub format: sha256=<hex>
        let sig_hex = signature.strip_prefix("sha256=").ok_or(SignatureError::InvalidFormat)?;

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::Crypto(e.to_string()))?;
        mac.update(payload);

        let expected = hex::encode(mac.finalize().into_bytes());

        Ok(constant_time_eq(sig_hex.as_bytes(), expected.as_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_signature(payload: &[u8], secret: &str) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload);
        format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
    }

    #[test]
    fn test_valid_signature() {
        let verifier = GitHubVerifier;
        let payload = b"test payload";
        let secret = "secret";
        let signature = generate_signature(payload, secret);

        assert!(verifier.verify(payload, &signature, secret, None).unwrap());
    }

    #[test]
    fn test_invalid_signature() {
        let verifier = GitHubVerifier;
        let signature = "sha256=invalid";

        assert!(!verifier.verify(b"test", signature, "secret", None).unwrap());
    }

    #[test]
    fn test_missing_prefix() {
        let verifier = GitHubVerifier;
        let result = verifier.verify(b"test", "abc123", "secret", None);
        assert!(matches!(result, Err(SignatureError::InvalidFormat)));
    }
}
