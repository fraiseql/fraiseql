//! Slack webhook signature verification.
//!
//! Format: `v0=<hex>` with timestamp in `X-Slack-Request-Timestamp` header.
//! Algorithm: HMAC-SHA256 of `v0:<timestamp>:<body>`
//!
//! Timestamps older than 5 minutes are rejected to prevent replay attacks.

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::{
    signature::{SignatureError, constant_time_eq},
    traits::SignatureVerifier,
};

/// Maximum age of a Slack webhook timestamp before it is considered a replay.
const MAX_TIMESTAMP_AGE_SECS: i64 = 300; // 5 minutes

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
        _url: Option<&str>,
    ) -> Result<bool, SignatureError> {
        // Slack format: v0=<hex>
        let sig_hex = signature.strip_prefix("v0=").ok_or(SignatureError::InvalidFormat)?;

        let timestamp = timestamp.ok_or(SignatureError::MissingTimestamp)?;

        // SECURITY: Reject replayed requests by checking timestamp freshness.
        // Timestamps older than MAX_TIMESTAMP_AGE_SECS are rejected.
        let ts_secs: i64 = timestamp.parse().map_err(|_| SignatureError::InvalidFormat)?;
        let now: i64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(i64::MAX, |d| d.as_secs() as i64);
        if (now - ts_secs).abs() > MAX_TIMESTAMP_AGE_SECS {
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

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use super::*;

    fn make_signature(timestamp: &str, payload: &[u8], secret: &str) -> String {
        let signed = format!("v0:{}:{}", timestamp, String::from_utf8_lossy(payload));
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(signed.as_bytes());
        format!("v0={}", hex::encode(mac.finalize().into_bytes()))
    }

    fn fresh_timestamp() -> String {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_string()
    }

    #[test]
    fn test_valid_signature() {
        let verifier = SlackVerifier;
        let payload = b"hello world";
        let secret = "8f742231b10e8888abcd99yyyzzz85a5";
        let ts = fresh_timestamp();
        let sig = make_signature(&ts, payload, secret);

        assert!(verifier.verify(payload, &sig, secret, Some(&ts), None).unwrap());
    }

    #[test]
    fn test_invalid_signature() {
        let verifier = SlackVerifier;
        let ts = fresh_timestamp();
        let result = verifier.verify(b"test", "v0=invalidsig", "secret", Some(&ts), None);
        assert!(matches!(result, Ok(false)));
    }

    #[test]
    fn test_missing_prefix() {
        let verifier = SlackVerifier;
        let ts = fresh_timestamp();
        let result = verifier.verify(b"test", "invalidsig", "secret", Some(&ts), None);
        assert!(matches!(result, Err(SignatureError::InvalidFormat)));
    }

    #[test]
    fn test_missing_timestamp() {
        let verifier = SlackVerifier;
        let result = verifier.verify(b"test", "v0=abc", "secret", None, None);
        assert!(matches!(result, Err(SignatureError::MissingTimestamp)));
    }

    #[test]
    fn test_expired_timestamp_rejected() {
        let verifier = SlackVerifier;
        // Timestamp 10 minutes in the past
        let old_ts = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 600)
            .to_string();
        let payload = b"payload";
        let secret = "secret";
        let sig = make_signature(&old_ts, payload, secret);

        let result = verifier.verify(payload, &sig, secret, Some(&old_ts), None);
        assert!(matches!(result, Err(SignatureError::TimestampExpired)));
    }
}
