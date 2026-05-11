#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::*;

fn generate_signature(payload: &[u8], secret: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(payload);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

#[test]
fn test_valid_signature() {
    let verifier = GitHubVerifier;
    let payload = b"test payload";
    let secret = "secret";
    let signature = generate_signature(payload, secret);

    assert!(verifier.verify(payload, &signature, secret, None, None).unwrap());
}

#[test]
fn test_invalid_signature() {
    let verifier = GitHubVerifier;
    let signature = "sha256=invalid";

    assert!(!verifier.verify(b"test", signature, "secret", None, None).unwrap());
}

#[test]
fn test_missing_prefix() {
    let verifier = GitHubVerifier;
    let result = verifier.verify(b"test", "abc123", "secret", None, None);
    assert!(matches!(result, Err(SignatureError::InvalidFormat)));
}
