//! Twilio webhook signature verification.
//!
//! Format: Base64 encoded HMAC-SHA1

use base64::{Engine as _, engine::general_purpose};
use hmac::{Hmac, Mac};
use sha1::Sha1;

use crate::webhooks::{
    signature::{SignatureError, constant_time_eq},
    traits::SignatureVerifier,
};

pub struct TwilioVerifier;

impl SignatureVerifier for TwilioVerifier {
    fn name(&self) -> &'static str {
        "twilio"
    }

    fn signature_header(&self) -> &'static str {
        "X-Twilio-Signature"
    }

    fn verify(
        &self,
        payload: &[u8],
        signature: &str,
        secret: &str,
        _timestamp: Option<&str>,
    ) -> Result<bool, SignatureError> {
        let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::Crypto(e.to_string()))?;
        mac.update(payload);

        let expected = general_purpose::STANDARD.encode(mac.finalize().into_bytes());

        Ok(constant_time_eq(signature.as_bytes(), expected.as_bytes()))
    }
}
