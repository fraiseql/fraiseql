//! Slack webhook signature verification.
//!
//! Format: `v0=<hex>` with timestamp in `X-Slack-Request-Timestamp` header.
//! Algorithm: HMAC-SHA256 of `v0:<timestamp>:<body>`
//!
//! Timestamps older than 5 minutes are rejected to prevent replay attacks.

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use crate::{
    request::InboundRequest,
    signature::{
        SignatureError, Verified, check_timestamp_freshness, constant_time_eq, system_now_secs,
        verified_if,
    },
    traits::SignatureVerifier,
};

/// Default maximum age of a Slack webhook timestamp before it is considered a replay.
const DEFAULT_TIMESTAMP_AGE_SECS: u64 = 300; // 5 minutes

/// Verifies Slack webhook signatures using HMAC-SHA256.
///
/// Slack signs `v0:<timestamp>:<body>` and sends `v0=<hex>` in the `X-Slack-Signature`
/// header, with the Unix timestamp in `X-Slack-Request-Timestamp`. Requests with
/// timestamps outside the tolerance window are rejected to prevent replay attacks.
pub struct SlackVerifier {
    /// Maximum acceptable age of a timestamp in seconds.
    tolerance_secs: u64,
}

impl SlackVerifier {
    /// Create a verifier with the default 5-minute timestamp tolerance.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tolerance_secs: DEFAULT_TIMESTAMP_AGE_SECS,
        }
    }

    /// Set a custom timestamp tolerance (in seconds).
    #[must_use]
    pub fn with_tolerance(mut self, seconds: u64) -> Self {
        self.tolerance_secs = seconds;
        self
    }
}

impl Default for SlackVerifier {
    fn default() -> Self {
        Self::new()
    }
}

/// The header this scheme reads its credential from.
const SIGNATURE_HEADER: &str = "X-Slack-Signature";

/// The header carrying the timestamp this scheme signs.
const TIMESTAMP_HEADER: &str = "X-Slack-Request-Timestamp";

impl SignatureVerifier for SlackVerifier {
    fn name(&self) -> &'static str {
        "slack"
    }

    fn verify(
        &self,
        request: &InboundRequest<'_>,
        secret: &str,
    ) -> Result<Verified, SignatureError> {
        let signature = request.require_header(SIGNATURE_HEADER)?;
        // Optional here, not required: the scheme's own `MissingTimestamp` is the
        // established answer for a delivery without one, and it is more specific than
        // "the credential is missing".
        let timestamp = request.header(TIMESTAMP_HEADER);
        let payload = request.body();
        if secret.is_empty() {
            return Err(SignatureError::KeyMaterial(
                "Slack signing secret must not be empty".to_string(),
            ));
        }
        // Slack format: v0=<hex>
        let sig_hex = signature.strip_prefix("v0=").ok_or(SignatureError::InvalidFormat)?;

        let timestamp = timestamp.ok_or(SignatureError::MissingTimestamp)?;

        // SECURITY: Reject replayed requests by checking timestamp freshness.
        check_timestamp_freshness(system_now_secs(), timestamp, self.tolerance_secs)?;

        // Signed payload: v0:<timestamp>:<body>
        let signed_payload = format!("v0:{}:{}", timestamp, String::from_utf8_lossy(payload));

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::KeyMaterial(e.to_string()))?;
        mac.update(signed_payload.as_bytes());

        let expected = hex::encode(mac.finalize().into_bytes());

        verified_if(constant_time_eq(sig_hex.as_bytes(), expected.as_bytes()))
    }
}

#[cfg(test)]
mod tests;
