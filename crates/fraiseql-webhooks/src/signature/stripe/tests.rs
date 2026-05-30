#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use super::*;
use crate::testing::mocks::MockClock;

fn generate_signature(payload: &str, secret: &str, timestamp: i64) -> String {
    let signed_payload = format!("{}.{}", timestamp, payload);
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(signed_payload.as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());
    format!("t={},v1={}", timestamp, sig)
}

#[test]
fn test_valid_signature() {
    let clock = Arc::new(MockClock::new(1_679_076_299));
    let verifier = StripeVerifier::with_clock(clock);
    let payload = b"test payload";
    let secret = "whsec_test";
    let signature = generate_signature(&String::from_utf8_lossy(payload), secret, 1_679_076_299);

    assert!(verifier.verify(payload, &signature, secret, None, None).unwrap());
}

#[test]
fn test_invalid_signature() {
    let clock = Arc::new(MockClock::new(1_679_076_299));
    let verifier = StripeVerifier::with_clock(clock);
    let signature = "t=1679076299,v1=invalid";

    assert!(!verifier.verify(b"test", signature, "secret", None, None).unwrap());
}

#[test]
fn test_expired_timestamp() {
    let clock = Arc::new(MockClock::new(1_679_076_299 + 600)); // 10 minutes later
    let verifier = StripeVerifier::with_clock(clock);
    let signature = generate_signature("test", "secret", 1_679_076_299);

    let result = verifier.verify(b"test", &signature, "secret", None, None);
    assert!(matches!(result, Err(SignatureError::TimestampExpired)));
}

#[test]
fn test_extract_timestamp() {
    let verifier = StripeVerifier::new();
    let signature = "t=1679076299,v1=abc123";
    assert_eq!(verifier.extract_timestamp(signature), Some(1_679_076_299));
}
