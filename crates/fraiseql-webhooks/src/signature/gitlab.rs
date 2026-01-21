//! GitLab webhook signature verification.
//!
//! Format: Plain token in X-Gitlab-Token header

use crate::signature::{constant_time_eq, SignatureError};
use crate::traits::SignatureVerifier;

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
    ) -> Result<bool, SignatureError> {
        // GitLab uses a simple token comparison
        Ok(constant_time_eq(signature.as_bytes(), secret.as_bytes()))
    }
}
