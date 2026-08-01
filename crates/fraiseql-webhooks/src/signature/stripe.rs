//! Stripe webhook signature verification.
//!
//! Format: `t=<timestamp>,v1=<signature>[,v1=<signature>...]` (one `v1` per active signing secret)
//! Algorithm: HMAC-SHA256
//! Signed payload: `<timestamp>.<payload>`

use std::sync::Arc;

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use crate::{
    signature::{SignatureError, check_timestamp_freshness, constant_time_eq},
    traits::{Clock, SignatureVerifier, SystemClock},
};

/// Verifies Stripe webhook signatures using HMAC-SHA256.
///
/// Stripe signs `<timestamp>.<body>` and sends the result in the `Stripe-Signature` header
/// as `t=<timestamp>,v1=<hex>`. Timestamps outside the tolerance window are rejected
/// to prevent replay attacks.
pub struct StripeVerifier {
    clock:     Arc<dyn Clock>,
    tolerance: u64,
}

impl StripeVerifier {
    /// Create a new verifier using the system clock and a 5-minute timestamp tolerance.
    #[must_use]
    pub fn new() -> Self {
        Self {
            clock:     Arc::new(SystemClock),
            tolerance: 300, // 5 minutes
        }
    }

    /// Create a new verifier with a custom `Clock` implementation, useful for testing.
    #[must_use]
    pub fn with_clock(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            tolerance: 300,
        }
    }

    /// Set the maximum acceptable age of a webhook timestamp in seconds.
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
        _url: Option<&str>,
    ) -> Result<bool, SignatureError> {
        if secret.is_empty() {
            return Err(SignatureError::Crypto(
                "Stripe webhook secret must not be empty".to_string(),
            ));
        }
        // Parse Stripe signature format: t=timestamp,v1=signature,v1=...
        //
        // Stripe sends one `v1` entry PER ACTIVE SIGNING SECRET, so during secret
        // rotation genuine deliveries carry several, in no guaranteed order. The
        // old parse collected into a map keyed on the scheme name, keeping only
        // the last `v1` — a delivery whose matching signature was not last was
        // rejected 401 for the whole rotation window (#787). Per Stripe's
        // verification guidance, the delivery is genuine when ANY candidate
        // matches (each candidate still compared in constant time).
        let mut timestamp = None;
        let mut v1_candidates = Vec::new();
        for part in signature.split(',') {
            let mut kv = part.splitn(2, '=');
            match (kv.next(), kv.next()) {
                (Some("t"), Some(value)) => timestamp = Some(value),
                (Some("v1"), Some(value)) => v1_candidates.push(value),
                _ => {},
            }
        }

        let timestamp = timestamp.ok_or(SignatureError::InvalidFormat)?;
        if v1_candidates.is_empty() {
            return Err(SignatureError::InvalidFormat);
        }

        // Verify timestamp is recent (replay protection) via the shared seam.
        check_timestamp_freshness(self.clock.now(), timestamp, self.tolerance)?;

        // Compute expected signature
        // signed_payload = timestamp + "." + payload
        let signed_payload = format!("{}.{}", timestamp, String::from_utf8_lossy(payload));

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::Crypto(e.to_string()))?;
        mac.update(signed_payload.as_bytes());

        let expected = hex::encode(mac.finalize().into_bytes());

        // Constant-time comparison per candidate; `|` (not `||`) so every
        // candidate is compared regardless of earlier matches.
        Ok(v1_candidates
            .iter()
            .fold(false, |acc, sig| acc | constant_time_eq(sig.as_bytes(), expected.as_bytes())))
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
mod tests;
