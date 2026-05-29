//! Stripe webhook signature verification.
//!
//! Format: `t=<timestamp>,v1=<signature>`
//! Algorithm: HMAC-SHA256
//! Signed payload: `<timestamp>.<payload>`

use std::{collections::HashMap, sync::Arc};

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use crate::{
    signature::{SignatureError, constant_time_eq},
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
        let ts: i64 = timestamp.parse().map_err(|_| SignatureError::InvalidFormat)?;

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
mod tests;
