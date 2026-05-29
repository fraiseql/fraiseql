//! Generic HMAC signature verifiers.

use hmac::{Hmac, KeyInit, Mac};
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
    /// Use [`Default::default()`] instead — this constructor exists only for
    /// backward compatibility. `name()` always returns `"hmac-sha256"` and
    /// `signature_header()` always returns `"X-Signature"` regardless of
    /// arguments, because the [`SignatureVerifier`] trait requires `&'static str`
    /// returns.
    #[must_use]
    pub fn new() -> Self {
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
    /// Use [`Default::default()`] instead. `name()` always returns `"hmac-sha1"` and
    /// `signature_header()` always returns `"X-Signature"`.
    #[must_use]
    pub fn new() -> Self {
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

#[cfg(test)]
mod tests;
