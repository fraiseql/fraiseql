//! Generic HMAC signature verifiers.

use crate::webhooks::signature::{constant_time_eq, SignatureError};
use crate::webhooks::traits::SignatureVerifier;
use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::Sha256;

/// Generic HMAC-SHA256 verifier with configurable header
pub struct HmacSha256Verifier {
    name: String,
    header: String,
}

impl HmacSha256Verifier {
    #[must_use]
    pub fn new(name: &str, header: &str) -> Self {
        Self {
            name: name.to_string(),
            header: header.to_string(),
        }
    }
}

impl Default for HmacSha256Verifier {
    fn default() -> Self {
        Self::new("hmac-sha256", "X-Signature")
    }
}

impl SignatureVerifier for HmacSha256Verifier {
    fn name(&self) -> &'static str {
        // This is a limitation - we'd need Box<str> or similar
        "hmac-sha256"
    }

    fn signature_header(&self) -> &'static str {
        // This is a limitation - we'd need Box<str> or similar
        "X-Signature"
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

        let expected = hex::encode(mac.finalize().into_bytes());

        Ok(constant_time_eq(signature.as_bytes(), expected.as_bytes()))
    }
}

/// Generic HMAC-SHA1 verifier with configurable header
pub struct HmacSha1Verifier {
    name: String,
    header: String,
}

impl HmacSha1Verifier {
    #[must_use]
    pub fn new(name: &str, header: &str) -> Self {
        Self {
            name: name.to_string(),
            header: header.to_string(),
        }
    }
}

impl Default for HmacSha1Verifier {
    fn default() -> Self {
        Self::new("hmac-sha1", "X-Signature")
    }
}

impl SignatureVerifier for HmacSha1Verifier {
    fn name(&self) -> &'static str {
        "hmac-sha1"
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
    ) -> Result<bool, SignatureError> {
        let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::Crypto(e.to_string()))?;
        mac.update(payload);

        let expected = hex::encode(mac.finalize().into_bytes());

        Ok(constant_time_eq(signature.as_bytes(), expected.as_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hmac_sha256() {
        let verifier = HmacSha256Verifier::default();
        let payload = b"test";
        let secret = "secret";

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload);
        let signature = hex::encode(mac.finalize().into_bytes());

        assert!(verifier.verify(payload, &signature, secret, None).unwrap());
    }

    #[test]
    fn test_hmac_sha1() {
        let verifier = HmacSha1Verifier::default();
        let payload = b"test";
        let secret = "secret";

        let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload);
        let signature = hex::encode(mac.finalize().into_bytes());

        assert!(verifier.verify(payload, &signature, secret, None).unwrap());
    }
}
