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

/// Verifies SendGrid (Twilio Email) event webhook signatures using ECDSA P-256 with SHA-256.
///
/// SendGrid signs `<timestamp><body>` with an ECDSA P-256 private key and sends the
/// Base64-encoded DER signature in `X-Twilio-Email-Event-Webhook-Signature` and the
/// timestamp in `X-Twilio-Email-Event-Webhook-Timestamp`. The `secret` parameter must
/// be the SendGrid ECDSA public key in PEM format.
pub struct SendGridVerifier;

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
        let ts = timestamp.unwrap_or("");
        let mut message = ts.as_bytes().to_vec();
        message.extend_from_slice(payload);

        // ECDSA P-256 with SHA-256 (p256 crate uses SHA-256 by default)
        Ok(public_key.verify(&message, &sig).is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_public_key_returns_error() {
        let verifier = SendGridVerifier;
        let result = verifier.verify(b"body", "sig", "not-a-pem-key", None, None);
        assert!(matches!(result, Err(SignatureError::Crypto(_))));
    }

    #[test]
    fn test_invalid_signature_base64() {
        let verifier = SendGridVerifier;
        // Use a real PEM key stub to get past key parsing
        let pem = concat!(
            "-----BEGIN PUBLIC KEY-----\n",
            "MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAE0OaghMQgGMiXbDEsGDFvZJeXRrwv\n",
            "oHSoitCAYeOSe9tqLl9xn7xbFvs5N2H+FzP9Y+sX7jlGRzW5/3D3OQ==\n",
            "-----END PUBLIC KEY-----\n"
        );
        let result = verifier.verify(b"body", "not-base64!!!", pem, None, None);
        assert!(matches!(result, Err(SignatureError::Crypto(_))));
    }
}
