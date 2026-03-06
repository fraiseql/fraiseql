//! GitLab webhook signature verification.
//!
//! Format: Plain token in X-Gitlab-Token header

use crate::{
    signature::{SignatureError, constant_time_eq},
    traits::SignatureVerifier,
};

/// Verifies GitLab webhook signatures using constant-time token comparison.
///
/// GitLab sends the configured secret token directly in the `X-Gitlab-Token` header.
/// No HMAC computation is involved; the header value is compared against the secret
/// using constant-time equality to prevent timing attacks.
pub struct GitLabVerifier;

impl SignatureVerifier for GitLabVerifier {
    fn name(&self) -> &'static str {
        "gitlab"
    }

    fn signature_header(&self) -> &'static str {
        "X-Gitlab-Token"
    }

    fn verify(
        &self,
        _payload: &[u8],
        signature: &str,
        secret: &str,
        _timestamp: Option<&str>,
        _url: Option<&str>,
    ) -> Result<bool, SignatureError> {
        // GitLab uses a simple token comparison
        Ok(constant_time_eq(signature.as_bytes(), secret.as_bytes()))
    }
}
