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
use p256::{
    ecdsa::{DerSignature, VerifyingKey, signature::Verifier as _},
    pkcs8::DecodePublicKey as _,
};

use crate::{
    request::InboundRequest,
    signature::{
        SignatureError, Verified, check_timestamp_freshness, system_now_secs, verified_if,
    },
    traits::SignatureVerifier,
};

/// Default maximum age of a SendGrid webhook timestamp before it is considered a replay.
const DEFAULT_TOLERANCE_SECS: u64 = 300; // 5 minutes

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
    tolerance_secs: u64,
}

impl SendGridVerifier {
    /// Create a verifier with the default 5-minute timestamp tolerance.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tolerance_secs: DEFAULT_TOLERANCE_SECS,
        }
    }

    /// Set a custom timestamp tolerance (in seconds).
    #[must_use]
    pub fn with_tolerance(mut self, seconds: u64) -> Self {
        self.tolerance_secs = seconds;
        self
    }
}

impl Default for SendGridVerifier {
    fn default() -> Self {
        Self::new()
    }
}

/// The header this scheme reads its credential from.
const SIGNATURE_HEADER: &str = "X-Twilio-Email-Event-Webhook-Signature";

/// The header carrying the timestamp this scheme signs.
const TIMESTAMP_HEADER: &str = "X-Twilio-Email-Event-Webhook-Timestamp";

impl SignatureVerifier for SendGridVerifier {
    fn name(&self) -> &'static str {
        "sendgrid"
    }

    fn verify(
        &self,
        request: &InboundRequest<'_>,
        secret: &str,
    ) -> Result<Verified, SignatureError> {
        let signature = request.require_header(SIGNATURE_HEADER)?;
        // Optional here, not required: the scheme's own `MissingTimestamp` is the
        // established answer for a delivery without one, and it is more specific than
        // "the credential is missing".
        let timestamp = request.header(TIMESTAMP_HEADER);
        let payload = request.body();
        if secret.is_empty() {
            return Err(SignatureError::KeyMaterial(
                "SendGrid public key must not be empty".to_string(),
            ));
        }

        // SECURITY: Timestamp is required — reject requests that omit it entirely.
        let ts = timestamp.ok_or(SignatureError::MissingTimestamp)?;

        // SECURITY: Validate timestamp freshness to prevent replay attacks.
        check_timestamp_freshness(system_now_secs(), ts, self.tolerance_secs)?;

        // Decode the PEM public key from `secret`.
        let public_key = VerifyingKey::from_public_key_pem(secret).map_err(|e| {
            SignatureError::KeyMaterial(format!("invalid SendGrid P-256 public key: {e}"))
        })?;

        // Decode the Base64-encoded DER signature. #1045: these two parse the *sender's*
        // header, so a failure is `InvalidFormat` (the sender's fault, 401) and never
        // `KeyMaterial` (the server's, 5xx) — otherwise any anonymous caller could mint a
        // 5xx on demand by posting an unparseable signature.
        let sig_der = general_purpose::STANDARD
            .decode(signature)
            .map_err(|_| SignatureError::InvalidFormat)?;

        let sig = DerSignature::try_from(sig_der.as_slice())
            .map_err(|_| SignatureError::InvalidFormat)?;

        // SendGrid signed message: timestamp_bytes + body_bytes
        let mut message = ts.as_bytes().to_vec();
        message.extend_from_slice(payload);

        // ECDSA P-256 with SHA-256 (p256 crate uses SHA-256 by default)
        verified_if(public_key.verify(&message, &sig).is_ok())
    }
}

#[cfg(test)]
mod tests;
