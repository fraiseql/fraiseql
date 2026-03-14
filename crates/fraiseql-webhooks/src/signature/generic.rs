//! Generic HMAC signature verifiers.

use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::Sha256;

use crate::{
    signature::{SignatureError, constant_time_eq},
    traits::SignatureVerifier,
};

/// Generic HMAC-SHA256 verifier with configurable header
///
/// **Note**: The `name` and `header` arguments to [`HmacSha256Verifier::new`]
/// are **not used** at runtime. The [`SignatureVerifier`] trait requires
/// `&'static str` return values from `name()` and `signature_header()`, so
/// this type always returns the defaults `"hmac-sha256"` and `"X-Signature"`.
/// Use [`Default::default()`] instead of `new()` to avoid confusion.
pub struct HmacSha256Verifier;

impl HmacSha256Verifier {
    /// Create a new verifier.
    ///
    /// # Ignored arguments
    ///
    /// The `name` and `header` arguments are **silently ignored** because the
    /// [`SignatureVerifier`] trait returns `&'static str`, making runtime
    /// configuration impossible. Both `name()` and `signature_header()` always
    /// return `"hmac-sha256"` and `"X-Signature"` respectively. Prefer
    /// [`Default::default()`] to make the intention explicit.
    #[must_use]
    pub fn new(_name: &str, _header: &str) -> Self {
        Self
    }
}

impl Default for HmacSha256Verifier {
    fn default() -> Self {
        Self
    }
}

impl SignatureVerifier for HmacSha256Verifier {
    fn name(&self) -> &'static str {
        "hmac-sha256"
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
            return Err(SignatureError::Crypto("HMAC-SHA256 secret must not be empty".to_string()));
        }
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::Crypto(e.to_string()))?;
        mac.update(payload);

        let expected = hex::encode(mac.finalize().into_bytes());

        Ok(constant_time_eq(signature.as_bytes(), expected.as_bytes()))
    }
}

/// Generic HMAC-SHA1 verifier with configurable header
///
/// **Note**: The `name` and `header` arguments to [`HmacSha1Verifier::new`]
/// are **not used** at runtime. Same limitation as [`HmacSha256Verifier`].
/// Prefer [`Default::default()`] to avoid confusion.
pub struct HmacSha1Verifier;

impl HmacSha1Verifier {
    /// Create a new verifier.
    ///
    /// # Ignored arguments
    ///
    /// See [`HmacSha256Verifier::new`] for the same limitation.
    #[must_use]
    pub fn new(_name: &str, _header: &str) -> Self {
        Self
    }
}

impl Default for HmacSha1Verifier {
    fn default() -> Self {
        Self
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
        _url: Option<&str>,
    ) -> Result<bool, SignatureError> {
        if secret.is_empty() {
            return Err(SignatureError::Crypto("HMAC-SHA1 secret must not be empty".to_string()));
        }
        let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::Crypto(e.to_string()))?;
        mac.update(payload);

        let expected = hex::encode(mac.finalize().into_bytes());

        Ok(constant_time_eq(signature.as_bytes(), expected.as_bytes()))
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hmac_sha256() {
        let verifier = HmacSha256Verifier;
        let payload = b"test";
        let secret = "secret";

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload);
        let signature = hex::encode(mac.finalize().into_bytes());

        assert!(verifier.verify(payload, &signature, secret, None, None).unwrap());
    }

    #[test]
    fn test_hmac_sha1() {
        let verifier = HmacSha1Verifier;
        let payload = b"test";
        let secret = "secret";

        let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload);
        let signature = hex::encode(mac.finalize().into_bytes());

        assert!(verifier.verify(payload, &signature, secret, None, None).unwrap());
    }
}
