#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use super::*;

/// The signature exactly as Lemon Squeezy produces it: PHP
/// `hash_hmac('sha256', $payload, $secret)` — **hex** output.
///
/// The previous version of this helper Base64-encoded, making the tests
/// self-consistent with the (wrong) verifier while every genuine delivery
/// bounced 401 (#781). The registry-level `genuine_delivery_fixtures` harness
/// exists so that class of drift is caught for every provider.
fn provider_signature(payload: &[u8], secret: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(payload);
    hex::encode(mac.finalize().into_bytes())
}

#[test]
fn test_valid_signature() {
    let verifier = LemonSqueezyVerifier;
    let payload = b"test payload";
    let secret = "secret";
    let signature = provider_signature(payload, secret);

    assert!(verifier.verify(payload, &signature, secret, None, None).unwrap());
}

#[test]
fn a_base64_signature_is_rejected() {
    // Pin the #781 fix direction: the old verifier accepted (only) its own
    // Base64 form, which no genuine delivery carries.
    use base64::{Engine as _, engine::general_purpose};
    let verifier = LemonSqueezyVerifier;
    let payload = b"test payload";
    let secret = "secret";
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(payload);
    let base64_signature = general_purpose::STANDARD.encode(mac.finalize().into_bytes());

    assert!(!verifier.verify(payload, &base64_signature, secret, None, None).unwrap());
}

#[test]
fn test_invalid_signature() {
    let verifier = LemonSqueezyVerifier;
    assert!(!verifier.verify(b"test", "invalid", "secret", None, None).unwrap());
}

#[test]
fn test_empty_secret_errors() {
    let verifier = LemonSqueezyVerifier;
    assert!(matches!(
        verifier.verify(b"test", "anything", "", None, None),
        Err(SignatureError::Crypto(_))
    ));
}
