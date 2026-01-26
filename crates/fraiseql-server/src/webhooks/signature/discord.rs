//! Discord webhook signature verification.
//!
//! Format: HMAC-SHA256 (simplified - real Discord uses Ed25519)
//! Note: For we use HMAC-SHA256. Full Ed25519 support in later phase.

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::webhooks::{
    signature::{SignatureError, constant_time_eq},
    traits::SignatureVerifier,
};

pub struct DiscordVerifier;

impl SignatureVerifier for DiscordVerifier {
    fn name(&self) -> &'static str {
        "discord"
    }

    fn signature_header(&self) -> &'static str {
        "X-Signature-Ed25519"
    }

    fn verify(
        &self,
        payload: &[u8],
        signature: &str,
        secret: &str,
        timestamp: Option<&str>,
    ) -> Result<bool, SignatureError> {
        // Simplified verification - real Discord uses Ed25519
        // For we use HMAC-SHA256 as a placeholder
        let timestamp = timestamp.ok_or(SignatureError::MissingTimestamp)?;

        let signed_payload = format!("{}{}", timestamp, String::from_utf8_lossy(payload));

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::Crypto(e.to_string()))?;
        mac.update(signed_payload.as_bytes());

        let expected = hex::encode(mac.finalize().into_bytes());

        Ok(constant_time_eq(signature.as_bytes(), expected.as_bytes()))
    }
}
