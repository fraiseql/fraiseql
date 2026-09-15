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
    timestamp: Option<&str>,
) -> Result<Verified, SignatureError> {
    let mut headers = BTreeMap::new();
    headers.insert("X-Slack-Signature".to_ascii_lowercase(), signature.to_string());
    if let Some(timestamp) = timestamp {
        headers.insert("X-Slack-Request-Timestamp".to_ascii_lowercase(), timestamp.to_string());
    }
    verifier.verify(&InboundRequest::new(&headers, payload, None), secret)
}

fn make_signature(timestamp: &str, payload: &[u8], secret: &str) -> String {
    let signed = format!("v0:{}:{}", timestamp, String::from_utf8_lossy(payload));
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(signed.as_bytes());
    format!("v0={}", hex::encode(mac.finalize().into_bytes()))
}

fn fresh_timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        .to_string()
}

#[test]
fn test_valid_signature() {
    let verifier = SlackVerifier::new();
    let payload = b"hello world";
    let secret = "8f742231b10e8888abcd99yyyzzz85a5";
    let ts = fresh_timestamp();
    let sig = make_signature(&ts, payload, secret);

    assert_eq!(check(&verifier, payload, &sig, secret, Some(&ts)).unwrap(), Verified::Body);
}

#[test]
fn test_invalid_signature() {
    let verifier = SlackVerifier::new();
    let ts = fresh_timestamp();
    let result = check(&verifier, b"test", "v0=invalidsig", "secret", Some(&ts));
    assert!(matches!(result, Err(SignatureError::Mismatch)));
}

#[test]
fn test_missing_prefix() {
    let verifier = SlackVerifier::new();
    let ts = fresh_timestamp();
    let result = check(&verifier, b"test", "invalidsig", "secret", Some(&ts));
    assert!(matches!(result, Err(SignatureError::InvalidFormat)));
}

#[test]
fn test_missing_timestamp() {
    let verifier = SlackVerifier::new();
    let result = check(&verifier, b"test", "v0=abc", "secret", None);
    assert!(matches!(result, Err(SignatureError::MissingTimestamp)));
}

#[test]
fn test_expired_timestamp_rejected() {
    let verifier = SlackVerifier::new();
    // Timestamp 10 minutes in the past
    let old_ts = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 600)
        .to_string();
    let payload = b"payload";
    let secret = "secret";
    let sig = make_signature(&old_ts, payload, secret);

    let result = check(&verifier, payload, &sig, secret, Some(&old_ts));
    assert!(matches!(result, Err(SignatureError::TimestampExpired)));
}
