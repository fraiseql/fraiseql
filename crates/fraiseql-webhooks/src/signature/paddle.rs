//! Paddle Billing v2 webhook signature verification.
//!
//! Format: `Paddle-Signature: ts=<unix_timestamp>;h1=<hex_hmac_sha256>`
//! Algorithm: HMAC-SHA256 of `<timestamp>:<body>` where timestamp is the `ts` value.
//!
//! See: <https://developer.paddle.com/webhooks/signature-verification>

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::{
    signature::{SignatureError, constant_time_eq},
    traits::SignatureVerifier,
};

/// Default maximum age of a Paddle webhook timestamp before it is considered a replay.
const DEFAULT_TOLERANCE_SECS: i64 = 300; // 5 minutes

/// Verifies Paddle Billing v2 webhook signatures using HMAC-SHA256.
///
/// Paddle signs `<timestamp>:<body>` and sends `ts=<timestamp>;h1=<hex>` in the
/// `Paddle-Signature` header. Timestamps outside the tolerance window are rejected
/// to prevent replay attacks.
pub struct PaddleVerifier {
    /// Maximum acceptable age of a timestamp in seconds.
    tolerance_secs: i64,
}

impl PaddleVerifier {
    /// Create a verifier with the default 5-minute timestamp tolerance.
    #[must_use]
    pub fn new() -> Self {
        Self { tolerance_secs: DEFAULT_TOLERANCE_SECS }
    }

    /// Set a custom timestamp tolerance (in seconds).
    #[must_use]
    pub fn with_tolerance(mut self, seconds: u64) -> Self {
        self.tolerance_secs = seconds as i64;
        self
    }
}

impl Default for PaddleVerifier {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse the Paddle v2 signature header.
///
/// Header format: `ts=<timestamp>;h1=<hex_hmac>`
///
/// Returns `(timestamp_str, hex_hmac)` or an error.
fn parse_paddle_signature(signature: &str) -> Result<(&str, &str), SignatureError> {
    let mut ts = None;
    let mut h1 = None;

    for part in signature.split(';') {
        if let Some(val) = part.strip_prefix("ts=") {
            ts = Some(val);
        } else if let Some(val) = part.strip_prefix("h1=") {
            h1 = Some(val);
        }
    }

    match (ts, h1) {
        (Some(t), Some(h)) => Ok((t, h)),
        _ => Err(SignatureError::InvalidFormat),
    }
}

impl SignatureVerifier for PaddleVerifier {
    fn name(&self) -> &'static str {
        "paddle"
    }

    fn signature_header(&self) -> &'static str {
        "Paddle-Signature"
    }

    fn verify(
        &self,
        payload: &[u8],
        signature: &str,
        secret: &str,
        _timestamp: Option<&str>,
        _url: Option<&str>,
    ) -> Result<bool, SignatureError> {
        if secret.is_empty() {
            return Err(SignatureError::Crypto(
                "Paddle webhook secret must not be empty".to_string(),
            ));
        }

        let (timestamp, h1_hex) = parse_paddle_signature(signature)?;

        // SECURITY: Validate timestamp freshness to prevent replay attacks.
        let ts_secs: i64 = timestamp.parse().map_err(|_| SignatureError::InvalidFormat)?;
        let now: i64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(i64::MAX, |d| d.as_secs() as i64);
        if (now - ts_secs).abs() > self.tolerance_secs {
            return Err(SignatureError::TimestampExpired);
        }

        // Paddle v2 signing string: "<timestamp>:<body>"
        let mut signing = timestamp.as_bytes().to_vec();
        signing.push(b':');
        signing.extend_from_slice(payload);

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::Crypto(e.to_string()))?;
        mac.update(&signing);

        let expected = hex::encode(mac.finalize().into_bytes());

        Ok(constant_time_eq(h1_hex.as_bytes(), expected.as_bytes()))
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_timestamp() -> String {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_string()
    }

    fn make_signature(timestamp: &str, payload: &[u8], secret: &str) -> String {
        let mut signing = timestamp.as_bytes().to_vec();
        signing.push(b':');
        signing.extend_from_slice(payload);

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(&signing);
        let h1 = hex::encode(mac.finalize().into_bytes());
        format!("ts={timestamp};h1={h1}")
    }

    #[test]
    fn test_valid_signature() {
        let verifier = PaddleVerifier::new();
        let payload = br#"{"event_type":"subscription.created"}"#;
        let secret = "pdl_ntfset_test_secret";
        let timestamp = fresh_timestamp();
        let sig = make_signature(&timestamp, payload, secret);

        assert!(verifier.verify(payload, &sig, secret, None, None).unwrap());
    }

    #[test]
    fn test_invalid_hmac() {
        let verifier = PaddleVerifier::new();
        let ts = fresh_timestamp();
        let sig = format!("ts={ts};h1=deadbeefdeadbeefdeadbeefdeadbeef");
        assert!(!verifier.verify(b"payload", &sig, "secret", None, None).unwrap());
    }

    #[test]
    fn test_invalid_format_missing_ts() {
        let verifier = PaddleVerifier::new();
        let result = verifier.verify(b"payload", "h1=abc123", "secret", None, None);
        assert!(matches!(result, Err(SignatureError::InvalidFormat)));
    }

    #[test]
    fn test_invalid_format_missing_h1() {
        let verifier = PaddleVerifier::new();
        let ts = fresh_timestamp();
        let sig = format!("ts={ts}");
        let result = verifier.verify(b"payload", &sig, "secret", None, None);
        assert!(matches!(result, Err(SignatureError::InvalidFormat)));
    }

    #[test]
    fn test_expired_timestamp_rejected() {
        let verifier = PaddleVerifier::new();
        let old_ts = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 600)
            .to_string();
        let payload = b"payload";
        let secret = "secret";
        let sig = make_signature(&old_ts, payload, secret);
        let result = verifier.verify(payload, &sig, secret, None, None);
        assert!(matches!(result, Err(SignatureError::TimestampExpired)));
    }

    #[test]
    fn test_empty_secret_rejected() {
        let verifier = PaddleVerifier::new();
        let ts = fresh_timestamp();
        let sig = format!("ts={ts};h1=abc123");
        let result = verifier.verify(b"payload", &sig, "", None, None);
        assert!(matches!(result, Err(SignatureError::Crypto(_))));
    }

    #[test]
    fn test_parse_signature_valid() {
        let (ts, h1) = parse_paddle_signature("ts=1234567890;h1=abc123def456").unwrap();
        assert_eq!(ts, "1234567890");
        assert_eq!(h1, "abc123def456");
    }

    #[test]
    fn test_parse_signature_extra_fields_ignored() {
        // Future-proofing: extra fields should not break parsing
        let (ts, h1) = parse_paddle_signature("ts=111;h2=ignored;h1=abc").unwrap();
        assert_eq!(ts, "111");
        assert_eq!(h1, "abc");
    }
}
