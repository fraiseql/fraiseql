//! Slack webhook signature verification.
//!
//! Format: `v0=<hex>` with timestamp in `X-Slack-Request-Timestamp` header.
//! Algorithm: HMAC-SHA256 of `v0:<timestamp>:<body>`
//!
//! Timestamps older than 5 minutes are rejected to prevent replay attacks.

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use crate::{
    signature::{SignatureError, constant_time_eq},
    traits::SignatureVerifier,
};

/// Default maximum age of a Slack webhook timestamp before it is considered a replay.
const DEFAULT_TIMESTAMP_AGE_SECS: i64 = 300; // 5 minutes

/// Verifies Slack webhook signatures using HMAC-SHA256.
///
/// Slack signs `v0:<timestamp>:<body>` and sends `v0=<hex>` in the `X-Slack-Signature`
/// header, with the Unix timestamp in `X-Slack-Request-Timestamp`. Requests with
/// timestamps outside the tolerance window are rejected to prevent replay attacks.
pub struct SlackVerifier {
    /// Maximum acceptable age of a timestamp in seconds.
    tolerance_secs: i64,
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
        self.tolerance_secs = seconds as i64;
        self
    }
}

impl Default for SlackVerifier {
    fn default() -> Self {
        Self::new()
    }
}

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
        _url: Option<&str>,
    ) -> Result<bool, SignatureError> {
        if secret.is_empty() {
            return Err(SignatureError::Crypto(
                "Slack signing secret must not be empty".to_string(),
            ));
        }
        // Slack format: v0=<hex>
        let sig_hex = signature.strip_prefix("v0=").ok_or(SignatureError::InvalidFormat)?;

        let timestamp = timestamp.ok_or(SignatureError::MissingTimestamp)?;

        // SECURITY: Reject replayed requests by checking timestamp freshness.
        let ts_secs: i64 = timestamp.parse().map_err(|_| SignatureError::InvalidFormat)?;
        let now: i64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(i64::MAX, |d| d.as_secs() as i64);
        if (now - ts_secs).abs() > self.tolerance_secs {
            return Err(SignatureError::TimestampExpired);
        }

        // Signed payload: v0:<timestamp>:<body>
        let signed_payload = format!("v0:{}:{}", timestamp, String::from_utf8_lossy(payload));

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::Crypto(e.to_string()))?;
        mac.update(signed_payload.as_bytes());

        let expected = hex::encode(mac.finalize().into_bytes());

        Ok(constant_time_eq(sig_hex.as_bytes(), expected.as_bytes()))
    }
}

#[cfg(test)]
mod tests;
