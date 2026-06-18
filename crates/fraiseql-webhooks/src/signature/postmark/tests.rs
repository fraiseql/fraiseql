#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use base64::{Engine as _, engine::general_purpose};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use super::*;

fn generate_signature(payload: &[u8], secret: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(payload);
    general_purpose::STANDARD.encode(mac.finalize().into_bytes())
}

#[test]
fn test_valid_signature() {
    let verifier = PostmarkVerifier;
    let payload = b"test payload";
    let secret = "secret";
    let signature = generate_signature(payload, secret);

    assert!(verifier.verify(payload, &signature, secret, None, None).unwrap());
}

#[test]
fn test_invalid_signature() {
    let verifier = PostmarkVerifier;
    assert!(!verifier.verify(b"test", "invalid", "secret", None, None).unwrap());
}

#[test]
fn test_empty_secret_errors() {
    let verifier = PostmarkVerifier;
    assert!(matches!(
        verifier.verify(b"test", "anything", "", None, None),
        Err(SignatureError::Crypto(_))
    ));
}
