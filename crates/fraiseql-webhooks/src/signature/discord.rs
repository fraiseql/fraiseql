//! Discord webhook signature verification.
//!
//! Discord uses Ed25519 signatures. The public key is provided by Discord
//! in the developer portal. The signature is sent in the X-Signature-Ed25519
//! header, with the timestamp in X-Signature-Timestamp.
//!
//! Timestamps older than 5 minutes are rejected to prevent replay attacks.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};

use crate::{
    signature::{SignatureError, check_timestamp_freshness, system_now_secs},
    traits::SignatureVerifier,
};

/// Default maximum age of a Discord webhook timestamp before it is considered a replay.
const DEFAULT_TIMESTAMP_AGE_SECS: u64 = 300; // 5 minutes

/// Verifies Discord webhook interaction signatures using Ed25519.
///
/// Discord signs `<timestamp><body>` with an Ed25519 private key and sends the hex-encoded
/// signature in `X-Signature-Ed25519` and the Unix timestamp in `X-Signature-Timestamp`.
/// The `secret` parameter must be the hex-encoded Ed25519 public key from the Discord
/// developer portal. Requests with timestamps outside the tolerance window are rejected.
pub struct DiscordVerifier {
    /// Maximum acceptable age of a timestamp in seconds.
    pub(crate) tolerance_secs: u64,
}

impl DiscordVerifier {
    /// Create a verifier with the default 5-minute timestamp tolerance.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tolerance_secs: DEFAULT_TIMESTAMP_AGE_SECS,
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

impl Default for DiscordVerifier {
    fn default() -> Self {
        Self::new()
    }
}

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
        _url: Option<&str>,
    ) -> Result<bool, SignatureError> {
        if secret.is_empty() {
            return Err(SignatureError::Crypto("Discord public key must not be empty".to_string()));
        }

        let timestamp = timestamp.ok_or(SignatureError::MissingTimestamp)?;

        // SECURITY: Reject replayed requests by checking timestamp freshness.
        // Discord timestamps are Unix seconds as decimal strings.
        check_timestamp_freshness(system_now_secs(), timestamp, self.tolerance_secs)?;

        // Decode the hex-encoded public key from secret
        let pk_bytes = hex::decode(secret)
            .map_err(|e| SignatureError::Crypto(format!("invalid public key hex: {e}")))?;

        let public_key = VerifyingKey::try_from(pk_bytes.as_slice())
            .map_err(|e| SignatureError::Crypto(format!("invalid Ed25519 public key: {e}")))?;

        // Decode the hex-encoded signature
        let sig_bytes = hex::decode(signature)
            .map_err(|e| SignatureError::Crypto(format!("invalid signature hex: {e}")))?;

        let sig = Signature::try_from(sig_bytes.as_slice())
            .map_err(|e| SignatureError::Crypto(format!("invalid Ed25519 signature: {e}")))?;

        // Discord signs: timestamp + body
        let mut message = timestamp.as_bytes().to_vec();
        message.extend_from_slice(payload);

        Ok(public_key.verify(&message, &sig).is_ok())
    }
}

#[cfg(test)]
mod tests;
