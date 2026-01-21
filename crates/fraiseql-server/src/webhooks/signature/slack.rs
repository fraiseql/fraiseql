//! Slack webhook signature verification.
//!
//! Format: `v0=<hex>` with timestamp in separate header
//! Algorithm: HMAC-SHA256 of `v0:<timestamp>:<body>`

use crate::webhooks::signature::{constant_time_eq, SignatureError};
use crate::webhooks::traits::SignatureVerifier;
use hmac::{Hmac, Mac};
use sha2::Sha256;

pub struct SlackVerifier;

impl SignatureVerifier for SlackVerifier {
    fn name(&self) -> &'static str {
        "slack"
    }

    fn signature_header(&self) -> &'static str {
        "X-Slack-Signature"
    }

    fn verify(
        &self,
        payload: &[u8],
        signature: &str,
        secret: &str,
        timestamp: Option<&str>,
    ) -> Result<bool, SignatureError> {
        // Slack format: v0=<hex>
        let sig_hex = signature
            .strip_prefix("v0=")
            .ok_or(SignatureError::InvalidFormat)?;

        let timestamp = timestamp.ok_or(SignatureError::MissingTimestamp)?;

        // Signed payload: v0:<timestamp>:<body>
        let signed_payload = format!("v0:{}:{}", timestamp, String::from_utf8_lossy(payload));

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::Crypto(e.to_string()))?;
        mac.update(signed_payload.as_bytes());

        let expected = hex::encode(mac.finalize().into_bytes());

        Ok(constant_time_eq(sig_hex.as_bytes(), expected.as_bytes()))
    }
}
