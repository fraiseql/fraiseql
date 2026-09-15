//! GitLab webhook signature verification.
//!
//! Format: Plain token in X-Gitlab-Token header

use crate::{
    request::InboundRequest,
    signature::{SignatureError, Verified, constant_time_eq, verified_if},
    traits::SignatureVerifier,
};

/// Verifies GitLab webhook signatures using constant-time token comparison.
///
/// GitLab sends the configured secret token directly in the `X-Gitlab-Token` header.
/// No HMAC computation is involved; the header value is compared against the secret
/// using constant-time equality to prevent timing attacks.
pub struct GitLabVerifier;

/// The header this scheme reads its credential from.
const TOKEN_HEADER: &str = "X-Gitlab-Token";

impl SignatureVerifier for GitLabVerifier {
    fn name(&self) -> &'static str {
        "gitlab"
    }

    fn verify(
        &self,
        request: &InboundRequest<'_>,
        secret: &str,
    ) -> Result<Verified, SignatureError> {
        let signature = request.require_header(TOKEN_HEADER)?;
        if secret.is_empty() {
            return Err(SignatureError::KeyMaterial(
                "GitLab webhook token must not be empty".to_string(),
            ));
        }
        // GitLab uses a simple token comparison
        verified_if(constant_time_eq(signature.as_bytes(), secret.as_bytes()))
    }
}

#[cfg(test)]
mod tests;
