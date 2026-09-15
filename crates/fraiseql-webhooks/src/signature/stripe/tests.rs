#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::{collections::BTreeMap, sync::Arc};

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use super::*;
use crate::testing::mocks::MockClock;

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
    headers.insert("Stripe-Signature".to_ascii_lowercase(), signature.to_string());
    verifier.verify(&InboundRequest::new(&headers, payload, None), secret)
}

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

    assert_eq!(check(&verifier, payload, &signature, secret).unwrap(), Verified::Body);
}

#[test]
fn test_invalid_signature() {
    let clock = Arc::new(MockClock::new(1_679_076_299));
    let verifier = StripeVerifier::with_clock(clock);
    let signature = "t=1679076299,v1=invalid";

    assert!(matches!(
        check(&verifier, b"test", signature, "secret"),
        Err(SignatureError::Mismatch)
    ));
}

#[test]
fn test_expired_timestamp() {
    let clock = Arc::new(MockClock::new(1_679_076_299 + 600)); // 10 minutes later
    let verifier = StripeVerifier::with_clock(clock);
    let signature = generate_signature("test", "secret", 1_679_076_299);

    let result = check(&verifier, b"test", &signature, "secret");
    assert!(matches!(result, Err(SignatureError::TimestampExpired)));
}

#[test]
fn test_extract_timestamp() {
    let verifier = StripeVerifier::new();
    let signature = "t=1679076299,v1=abc123";
    assert_eq!(verifier.extract_timestamp(signature), Some(1_679_076_299));
}
