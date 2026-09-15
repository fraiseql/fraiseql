//! Shopify webhook signature verification.
//!
//! Format: Base64 encoded HMAC-SHA256

use base64::{Engine as _, engine::general_purpose};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use crate::{
    request::InboundRequest,
    signature::{SignatureError, Verified, constant_time_eq, verified_if},
    traits::SignatureVerifier,
};

/// Verifies Shopify webhook signatures using HMAC-SHA256 encoded as Base64.
///
/// Shopify computes `HMAC-SHA256(secret, body)`, Base64-encodes the result, and
/// sends it in the `X-Shopify-Hmac-Sha256` header.
pub struct ShopifyVerifier;

/// The header this scheme reads its credential from.
const SIGNATURE_HEADER: &str = "X-Shopify-Hmac-Sha256";

impl SignatureVerifier for ShopifyVerifier {
    fn name(&self) -> &'static str {
        "shopify"
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
                "Shopify webhook secret must not be empty".to_string(),
            ));
        }
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::KeyMaterial(e.to_string()))?;
        mac.update(payload);

        let expected = general_purpose::STANDARD.encode(mac.finalize().into_bytes());

        verified_if(constant_time_eq(signature.as_bytes(), expected.as_bytes()))
    }
}

#[cfg(test)]
mod tests;
