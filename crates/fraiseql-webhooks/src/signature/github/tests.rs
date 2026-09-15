#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::collections::BTreeMap;

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use super::*;

/// Drive the scheme the way the route does: put the credential under the header
/// the scheme reads, and hand it the whole request.
///
/// The argument order is the one `verify` had before #1321, so the rewrite of
/// these call sites carried no judgement about which value is which.
fn check(
    verifier: &impl SignatureVerifier,
    payload: &[u8],
    signature: &str,
    secret: &str,
) -> Result<Verified, SignatureError> {
    let mut headers = BTreeMap::new();
    headers.insert("X-Hub-Signature-256".to_ascii_lowercase(), signature.to_string());
    verifier.verify(&InboundRequest::new(&headers, payload, None), secret)
}

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

    assert_eq!(check(&verifier, payload, &signature, secret).unwrap(), Verified::Body);
}

#[test]
fn test_invalid_signature() {
    let verifier = GitHubVerifier;
    let signature = "sha256=invalid";

    assert!(matches!(
        check(&verifier, b"test", signature, "secret"),
        Err(SignatureError::Mismatch)
    ));
}

#[test]
fn test_missing_prefix() {
    let verifier = GitHubVerifier;
    let result = check(&verifier, b"test", "abc123", "secret");
    assert!(matches!(result, Err(SignatureError::InvalidFormat)));
}
