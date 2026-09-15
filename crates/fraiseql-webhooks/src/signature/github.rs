//! GitHub webhook signature verification.
//!
//! Format: `sha256=<hex>`
//! Algorithm: HMAC-SHA256

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use crate::{
    request::InboundRequest,
    signature::{SignatureError, Verified, constant_time_eq, verified_if},
    traits::SignatureVerifier,
};

/// Verifies GitHub webhook signatures using HMAC-SHA256.
///
/// GitHub computes `HMAC-SHA256(secret, body)` and sends it as `sha256=<hex>`
/// in the `X-Hub-Signature-256` header.
pub struct GitHubVerifier;

/// The header this scheme reads its credential from.
const SIGNATURE_HEADER: &str = "X-Hub-Signature-256";

impl SignatureVerifier for GitHubVerifier {
    fn name(&self) -> &'static str {
        "github"
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
                "GitHub webhook secret must not be empty".to_string(),
            ));
        }
        // GitHub format: sha256=<hex>
        let sig_hex = signature.strip_prefix("sha256=").ok_or(SignatureError::InvalidFormat)?;

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::KeyMaterial(e.to_string()))?;
        mac.update(payload);

        let expected = hex::encode(mac.finalize().into_bytes());

        verified_if(constant_time_eq(sig_hex.as_bytes(), expected.as_bytes()))
    }
}

#[cfg(test)]
mod tests;
