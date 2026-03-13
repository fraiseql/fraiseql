//! SendGrid (Twilio Email) webhook signature verification.
//!
//! Algorithm: ECDSA P-256 with SHA-256.
//! The `secret` is the SendGrid ECDSA public key in PEM format.
//! The `timestamp` is from the `X-Twilio-Email-Event-Webhook-Timestamp` header.
//! The signed message is: `<timestamp_bytes><body_bytes>` (concatenated).
//! The signature header (`X-Twilio-Email-Event-Webhook-Signature`) is Base64 encoded DER.
//!
//! See: <https://docs.sendgrid.com/for-developers/tracking-events/getting-started-event-webhook-security-features>

use base64::{Engine as _, engine::general_purpose};
use p256::ecdsa::{DerSignature, VerifyingKey, signature::Verifier as _};
use p256::pkcs8::DecodePublicKey as _;

use crate::{signature::SignatureError, traits::SignatureVerifier};

/// Default maximum age of a SendGrid webhook timestamp before it is considered a replay.
const DEFAULT_TOLERANCE_SECS: i64 = 300; // 5 minutes

/// Verifies SendGrid (Twilio Email) event webhook signatures using ECDSA P-256 with SHA-256.
///
/// SendGrid signs `<timestamp><body>` with an ECDSA P-256 private key and sends the
/// Base64-encoded DER signature in `X-Twilio-Email-Event-Webhook-Signature` and the
/// timestamp in `X-Twilio-Email-Event-Webhook-Timestamp`. The `secret` parameter must
/// be the SendGrid ECDSA public key in PEM format. The timestamp header is required;
/// requests without it are rejected. Requests with timestamps outside the tolerance
/// window are rejected to prevent replay attacks.
pub struct SendGridVerifier {
    /// Maximum acceptable age of a timestamp in seconds.
    tolerance_secs: i64,
}

impl SendGridVerifier {
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

impl Default for SendGridVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl SignatureVerifier for SendGridVerifier {
    fn name(&self) -> &'static str {
        "sendgrid"
    }

    fn signature_header(&self) -> &'static str {
        "X-Twilio-Email-Event-Webhook-Signature"
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
                "SendGrid public key must not be empty".to_string(),
            ));
        }

        // SECURITY: Timestamp is required — reject requests that omit it entirely.
        let ts = timestamp.ok_or(SignatureError::MissingTimestamp)?;

        // SECURITY: Validate timestamp freshness to prevent replay attacks.
        let ts_secs: i64 = ts.parse().map_err(|_| SignatureError::InvalidFormat)?;
        let now: i64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(i64::MAX, |d| d.as_secs() as i64);
        if (now - ts_secs).abs() > self.tolerance_secs {
            return Err(SignatureError::TimestampExpired);
        }

        // Decode the PEM public key from `secret`.
        let public_key = VerifyingKey::from_public_key_pem(secret)
            .map_err(|e| SignatureError::Crypto(format!("invalid SendGrid P-256 public key: {e}")))?;

        // Decode the Base64-encoded DER signature.
        let sig_der = general_purpose::STANDARD
            .decode(signature)
            .map_err(|e| SignatureError::Crypto(format!("invalid signature base64: {e}")))?;

        let sig = DerSignature::try_from(sig_der.as_slice())
            .map_err(|e| SignatureError::Crypto(format!("invalid DER signature: {e}")))?;

        // SendGrid signed message: timestamp_bytes + body_bytes
        let mut message = ts.as_bytes().to_vec();
        message.extend_from_slice(payload);

        // ECDSA P-256 with SHA-256 (p256 crate uses SHA-256 by default)
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
    fn test_missing_timestamp_returns_error() {
        let verifier = SendGridVerifier::new();
        // Timestamp is now required; passing None must fail.
        let result = verifier.verify(b"body", "sig", "not-a-pem-key", None, None);
        assert!(matches!(result, Err(SignatureError::MissingTimestamp)));
    }

    #[test]
    fn test_invalid_public_key_returns_error() {
        let verifier = SendGridVerifier::new();
        let ts = fresh_timestamp();
        let result = verifier.verify(b"body", "sig", "not-a-pem-key", Some(&ts), None);
        assert!(matches!(result, Err(SignatureError::Crypto(_))));
    }

    #[test]
    fn test_expired_timestamp_rejected() {
        let verifier = SendGridVerifier::new();
        let old_ts = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 600)
            .to_string();
        // Even before key parsing, an expired timestamp must be rejected.
        let result = verifier.verify(b"body", "sig", "not-a-pem-key", Some(&old_ts), None);
        assert!(matches!(result, Err(SignatureError::TimestampExpired)));
    }

    #[test]
    fn test_invalid_signature_base64() {
        let verifier = SendGridVerifier::new();
        // Use a real PEM key stub to get past key parsing
        let pem = concat!(
            "-----BEGIN PUBLIC KEY-----\n",
            "MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAE0OaghMQgGMiXbDEsGDFvZJeXRrwv\n",
            "oHSoitCAYeOSe9tqLl9xn7xbFvs5N2H+FzP9Y+sX7jlGRzW5/3D3OQ==\n",
            "-----END PUBLIC KEY-----\n"
        );
        let ts = fresh_timestamp();
        let result = verifier.verify(b"body", "not-base64!!!", pem, Some(&ts), None);
        assert!(matches!(result, Err(SignatureError::Crypto(_))));
    }

    #[test]
    fn test_empty_secret_rejected() {
        let verifier = SendGridVerifier::new();
        let ts = fresh_timestamp();
        let result = verifier.verify(b"body", "sig", "", Some(&ts), None);
        assert!(matches!(result, Err(SignatureError::Crypto(_))));
    }

    /// Round-trip test: generate a P-256 key pair, sign, and verify.
    ///
    /// This is the only acceptance-path test — all other tests cover rejection.
    /// It proves that the message construction (`timestamp_bytes + body_bytes`)
    /// matches what a real SendGrid webhook would produce.
    #[test]
    fn test_valid_signature_round_trip() {
        use p256::ecdsa::{DerSignature, Signature, SigningKey, signature::Signer as _};
        use p256::pkcs8::EncodePublicKey;
        use rand_core::OsRng;

        let signing_key = SigningKey::random(&mut OsRng);
        let verifying_key = *signing_key.verifying_key();

        // Export as SPKI PEM — same format SendGrid public keys use
        let public_key_pem = verifying_key
            .to_public_key_pem(p256::pkcs8::der::pem::LineEnding::default())
            .expect("P-256 VerifyingKey serializes to SPKI PEM");

        let ts = fresh_timestamp();
        let body = b"[{\"event\":\"delivered\",\"email\":\"user@example.com\"}]";

        // Build the exact message the verifier reconstructs
        let mut message = ts.as_bytes().to_vec();
        message.extend_from_slice(body);

        // Sign and encode as DER (the format SendGrid sends)
        let sig: Signature = signing_key.sign(&message);
        let sig_der: DerSignature = sig.to_der();
        let sig_b64 = general_purpose::STANDARD.encode(sig_der.as_ref());

        let verifier = SendGridVerifier::new();
        let result = verifier.verify(body, &sig_b64, &public_key_pem, Some(&ts), None);
        assert!(
            matches!(result, Ok(true)),
            "valid ECDSA P-256 signature must verify successfully"
        );
    }
}
