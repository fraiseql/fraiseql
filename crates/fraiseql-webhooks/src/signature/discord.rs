//! Discord webhook signature verification.
//!
//! Discord uses Ed25519 signatures. The public key is provided by Discord
//! in the developer portal. The signature is sent in the X-Signature-Ed25519
//! header, with the timestamp in X-Signature-Timestamp.
//!
//! Timestamps older than 5 minutes are rejected to prevent replay attacks.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};

use crate::{signature::SignatureError, traits::SignatureVerifier};

/// Default maximum age of a Discord webhook timestamp before it is considered a replay.
const DEFAULT_TIMESTAMP_AGE_SECS: i64 = 300; // 5 minutes

pub struct DiscordVerifier {
    /// Maximum acceptable age of a timestamp in seconds.
    tolerance_secs: i64,
}

impl DiscordVerifier {
    /// Create a verifier with the default 5-minute timestamp tolerance.
    #[must_use]
    pub fn new() -> Self {
        Self { tolerance_secs: DEFAULT_TIMESTAMP_AGE_SECS }
    }

    /// Set a custom timestamp tolerance (in seconds).
    #[must_use]
    pub fn with_tolerance(mut self, seconds: u64) -> Self {
        self.tolerance_secs = seconds as i64;
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
        let timestamp = timestamp.ok_or(SignatureError::MissingTimestamp)?;

        // SECURITY: Reject replayed requests by checking timestamp freshness.
        // Discord timestamps are Unix seconds as decimal strings.
        let ts_secs: i64 = timestamp.parse().map_err(|_| SignatureError::InvalidFormat)?;
        let now: i64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(i64::MAX, |d| d.as_secs() as i64);
        if (now - ts_secs).abs() > self.tolerance_secs {
            return Err(SignatureError::TimestampExpired);
        }

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

    #[test]
    fn test_missing_timestamp() {
        let verifier = DiscordVerifier::new();
        let result = verifier.verify(b"test", "abc", "deadbeef", None, None);
        assert!(matches!(result, Err(SignatureError::MissingTimestamp)));
    }

    #[test]
    fn test_expired_timestamp_rejected() {
        let verifier = DiscordVerifier::new();
        let old_ts = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 600)
            .to_string();
        // Even with a valid signature format, an old timestamp should be rejected.
        let result = verifier.verify(b"payload", "deadbeef", "deadbeef", Some(&old_ts), None);
        assert!(matches!(result, Err(SignatureError::TimestampExpired)));
    }

    #[test]
    fn test_invalid_public_key_hex() {
        let verifier = DiscordVerifier::new();
        let ts = fresh_timestamp();
        let result = verifier.verify(b"test", "abc123", "not-hex!", Some(&ts), None);
        assert!(matches!(result, Err(SignatureError::Crypto(_))));
    }
}
