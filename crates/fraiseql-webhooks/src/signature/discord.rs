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

/// Verifies Discord webhook interaction signatures using Ed25519.
///
/// Discord signs `<timestamp><body>` with an Ed25519 private key and sends the hex-encoded
/// signature in `X-Signature-Ed25519` and the Unix timestamp in `X-Signature-Timestamp`.
/// The `secret` parameter must be the hex-encoded Ed25519 public key from the Discord
/// developer portal. Requests with timestamps outside the tolerance window are rejected.
pub struct DiscordVerifier {
    /// Maximum acceptable age of a timestamp in seconds.
    tolerance_secs: i64,
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
    /// Values that exceed [`i64::MAX`] are clamped to [`i64::MAX`] (≈ 292 billion years —
    /// effectively infinite tolerance).  A raw `seconds as i64` cast would silently wrap
    /// for large inputs, potentially yielding a *negative* tolerance that rejects every
    /// timestamp, disabling replay protection in an unexpected direction.
    #[must_use]
    pub fn with_tolerance(mut self, seconds: u64) -> Self {
        self.tolerance_secs = i64::try_from(seconds).unwrap_or(i64::MAX);
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
    use ed25519_dalek::{Signer, SigningKey};

    use super::*;

    /// Deterministic test seed — avoids `OsRng` in unit tests for reproducibility.
    const TEST_KEY_SEED: [u8; 32] = [
        0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c,
        0x44, 0xda, 0x08, 0x64, 0x1e, 0xea, 0x2a, 0x4f, 0xc5, 0x38, 0xe0, 0x17, 0xd5, 0x86, 0x64,
        0x6e, 0xa6,
    ];

    fn fresh_timestamp() -> String {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_string()
    }

    /// Build a signing key from a fixed seed, sign `timestamp + payload`, and return
    /// `(hex_public_key, hex_signature)`.
    fn make_valid_discord_signature(timestamp: &str, payload: &[u8]) -> (String, String) {
        let signing_key = SigningKey::from_bytes(&TEST_KEY_SEED);
        let verifying_key = signing_key.verifying_key();

        let mut message = timestamp.as_bytes().to_vec();
        message.extend_from_slice(payload);

        let signature = signing_key.sign(&message);
        (hex::encode(verifying_key.as_bytes()), hex::encode(signature.to_bytes()))
    }

    #[test]
    fn test_valid_signature_accepted() {
        let verifier = DiscordVerifier::new();
        let ts = fresh_timestamp();
        let payload = br#"{"type":1}"#;
        let (public_key_hex, sig_hex) = make_valid_discord_signature(&ts, payload);

        let result = verifier.verify(payload, &sig_hex, &public_key_hex, Some(&ts), None);
        assert!(
            matches!(result, Ok(true)),
            "valid Ed25519 signature should be accepted; got: {result:?}"
        );
    }

    #[test]
    fn test_tampered_payload_rejected() {
        let verifier = DiscordVerifier::new();
        let ts = fresh_timestamp();
        let (public_key_hex, sig_hex) = make_valid_discord_signature(&ts, br#"{"type":1}"#);

        // Different payload — signature is no longer valid.
        let result = verifier.verify(b"tampered", &sig_hex, &public_key_hex, Some(&ts), None);
        assert!(
            matches!(result, Ok(false)),
            "tampered payload should be rejected; got: {result:?}"
        );
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

    #[test]
    fn test_with_tolerance_large_value_does_not_wrap() {
        // A raw `u64::MAX as i64` cast wraps to -1, making every timestamp "expired".
        // `i64::try_from().unwrap_or(i64::MAX)` must clamp instead.
        let verifier = DiscordVerifier::new().with_tolerance(u64::MAX);
        assert!(
            verifier.tolerance_secs > 0,
            "tolerance_secs must not wrap to negative for u64::MAX input; \
             got {}",
            verifier.tolerance_secs
        );
        assert_eq!(verifier.tolerance_secs, i64::MAX);
    }
}
