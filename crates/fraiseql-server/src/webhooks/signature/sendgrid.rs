//! SendGrid webhook signature verification.
//!
//! Format: HMAC-SHA256 hex encoded

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::webhooks::{
    signature::{SignatureError, constant_time_eq},
    traits::SignatureVerifier,
};

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
        _timestamp: Option<&str>,
    ) -> Result<bool, SignatureError> {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::Crypto(e.to_string()))?;
        mac.update(payload);

        let expected = hex::encode(mac.finalize().into_bytes());

        Ok(constant_time_eq(signature.as_bytes(), expected.as_bytes()))
    }
}
