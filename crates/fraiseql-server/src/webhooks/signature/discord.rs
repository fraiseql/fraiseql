//! Discord webhook signature verification.
//!
//! Discord uses Ed25519 signatures. The public key is provided by Discord
//! in the developer portal. The signature is sent in the X-Signature-Ed25519
//! header, with the timestamp in X-Signature-Timestamp.

use ed25519_dalek::{Signature, VerifyingKey, Verifier};

use crate::webhooks::{
    signature::SignatureError,
    traits::SignatureVerifier,
};

pub struct DiscordVerifier;

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
    ) -> Result<bool, SignatureError> {
        let timestamp = timestamp.ok_or(SignatureError::MissingTimestamp)?;

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
