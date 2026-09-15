//! Paddle Billing v2 webhook signature verification.
//!
//! Format: `Paddle-Signature: ts=<unix_timestamp>;h1=<hex_hmac_sha256>`
//! Algorithm: HMAC-SHA256 of `<timestamp>:<body>` where timestamp is the `ts` value.
//!
//! See: <https://developer.paddle.com/webhooks/signature-verification>

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

/// Default maximum age of a Paddle webhook timestamp before it is considered a replay.
const DEFAULT_TOLERANCE_SECS: u64 = 300; // 5 minutes

/// Verifies Paddle Billing v2 webhook signatures using HMAC-SHA256.
///
/// Paddle signs `<timestamp>:<body>` and sends `ts=<timestamp>;h1=<hex>` in the
/// `Paddle-Signature` header. Timestamps outside the tolerance window are rejected
/// to prevent replay attacks.
pub struct PaddleVerifier {
    /// Maximum acceptable age of a timestamp in seconds.
    tolerance_secs: u64,
}

impl PaddleVerifier {
    /// Create a verifier with the default 5-minute timestamp tolerance.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tolerance_secs: DEFAULT_TOLERANCE_SECS,
        }
    }

    /// Set a custom timestamp tolerance (in seconds).
    ///
    /// The value is stored verbatim; the shared `check_timestamp_freshness`
    /// saturates it to `i64::MAX` at comparison time, so a large tolerance can
    /// never wrap to a negative window.
    #[must_use]
    pub fn with_tolerance(mut self, seconds: u64) -> Self {
        self.tolerance_secs = seconds;
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
pub(crate) fn parse_paddle_signature(signature: &str) -> Result<(&str, &str), SignatureError> {
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

/// The header this scheme reads its credential from.
const SIGNATURE_HEADER: &str = "Paddle-Signature";

impl SignatureVerifier for PaddleVerifier {
    fn name(&self) -> &'static str {
        "paddle"
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
                "Paddle webhook secret must not be empty".to_string(),
            ));
        }

        let (timestamp, h1_hex) = parse_paddle_signature(signature)?;

        // SECURITY: Validate timestamp freshness to prevent replay attacks.
        check_timestamp_freshness(system_now_secs(), timestamp, self.tolerance_secs)?;

        // Paddle v2 signing string: "<timestamp>:<body>"
        let mut signing = timestamp.as_bytes().to_vec();
        signing.push(b':');
        signing.extend_from_slice(payload);

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::KeyMaterial(e.to_string()))?;
        mac.update(&signing);

        let expected = hex::encode(mac.finalize().into_bytes());

        verified_if(constant_time_eq(h1_hex.as_bytes(), expected.as_bytes()))
    }
}

#[cfg(test)]
mod tests;
