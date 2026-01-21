//! Stripe webhook signature verification.
//!
//! Format: `t=<timestamp>,v1=<signature>`
//! Algorithm: HMAC-SHA256
//! Signed payload: `<timestamp>.<payload>`

use crate::webhooks::signature::{constant_time_eq, SignatureError};
use crate::webhooks::traits::{Clock, SignatureVerifier, SystemClock};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::Arc;

pub struct StripeVerifier {
    clock: Arc<dyn Clock>,
    tolerance: u64,
}

impl StripeVerifier {
    #[must_use]
    pub fn new() -> Self {
        Self {
            clock: Arc::new(SystemClock),
            tolerance: 300, // 5 minutes
        }
    }

    #[must_use]
    pub fn with_clock(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            tolerance: 300,
        }
    }

    #[must_use]
    pub fn with_tolerance(mut self, seconds: u64) -> Self {
        self.tolerance = seconds;
        self
    }
}

impl Default for StripeVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl SignatureVerifier for StripeVerifier {
    fn name(&self) -> &'static str {
        "stripe"
    }

    fn signature_header(&self) -> &'static str {
        "Stripe-Signature"
    }

    fn verify(
        &self,
        payload: &[u8],
        signature: &str,
        secret: &str,
        _timestamp: Option<&str>,
    ) -> Result<bool, SignatureError> {
        // Parse Stripe signature format: t=timestamp,v1=signature
        let parts: HashMap<&str, &str> = signature
            .split(',')
            .filter_map(|part| {
                let mut kv = part.splitn(2, '=');
                Some((kv.next()?, kv.next()?))
            })
            .collect();

        let timestamp = parts.get("t").ok_or(SignatureError::InvalidFormat)?;

        let sig_v1 = parts.get("v1").ok_or(SignatureError::InvalidFormat)?;

        // Verify timestamp is recent
        let ts: i64 = timestamp
            .parse()
            .map_err(|_| SignatureError::InvalidFormat)?;

        let now = self.clock.now();

        if (now - ts).abs() > self.tolerance as i64 {
            return Err(SignatureError::TimestampExpired);
        }

        // Compute expected signature
        // signed_payload = timestamp + "." + payload
        let signed_payload = format!("{}.{}", timestamp, String::from_utf8_lossy(payload));

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::Crypto(e.to_string()))?;
        mac.update(signed_payload.as_bytes());

        let expected = hex::encode(mac.finalize().into_bytes());

        // Constant-time comparison
        Ok(constant_time_eq(sig_v1.as_bytes(), expected.as_bytes()))
    }

    fn extract_timestamp(&self, signature: &str) -> Option<i64> {
        signature
            .split(',')
            .find(|p| p.starts_with("t="))
            .and_then(|p| p.strip_prefix("t="))
            .and_then(|t| t.parse().ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webhooks::testing::mocks::MockClock;

    fn generate_signature(payload: &str, secret: &str, timestamp: i64) -> String {
        let signed_payload = format!("{}.{}", timestamp, payload);
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(signed_payload.as_bytes());
        let sig = hex::encode(mac.finalize().into_bytes());
        format!("t={},v1={}", timestamp, sig)
    }

    #[test]
    fn test_valid_signature() {
        let clock = Arc::new(MockClock::new(1679076299));
        let verifier = StripeVerifier::with_clock(clock);
        let payload = b"test payload";
        let secret = "whsec_test";
        let signature = generate_signature(
            &String::from_utf8_lossy(payload),
            secret,
            1679076299,
        );

        assert!(verifier.verify(payload, &signature, secret, None).unwrap());
    }

    #[test]
    fn test_invalid_signature() {
        let clock = Arc::new(MockClock::new(1679076299));
        let verifier = StripeVerifier::with_clock(clock);
        let signature = "t=1679076299,v1=invalid";

        assert!(!verifier
            .verify(b"test", &signature, "secret", None)
            .unwrap());
    }

    #[test]
    fn test_expired_timestamp() {
        let clock = Arc::new(MockClock::new(1679076299 + 600)); // 10 minutes later
        let verifier = StripeVerifier::with_clock(clock);
        let signature = generate_signature("test", "secret", 1679076299);

        let result = verifier.verify(b"test", &signature, "secret", None);
        assert!(matches!(result, Err(SignatureError::TimestampExpired)));
    }

    #[test]
    fn test_extract_timestamp() {
        let verifier = StripeVerifier::new();
        let signature = "t=1679076299,v1=abc123";
        assert_eq!(verifier.extract_timestamp(signature), Some(1679076299));
    }
}
