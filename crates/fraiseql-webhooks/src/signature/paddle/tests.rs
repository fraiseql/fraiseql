#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::panic)] // Reason: test module — a `let ... else` that cannot bind the expected error variant must fail loudly and say what it got (#1174)

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
    headers.insert("Paddle-Signature".to_ascii_lowercase(), signature.to_string());
    verifier.verify(&InboundRequest::new(&headers, payload, None), secret)
}

fn fresh_timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        .to_string()
}

fn make_signature(timestamp: &str, payload: &[u8], secret: &str) -> String {
    let mut signing = timestamp.as_bytes().to_vec();
    signing.push(b':');
    signing.extend_from_slice(payload);

    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(&signing);
    let h1 = hex::encode(mac.finalize().into_bytes());
    format!("ts={timestamp};h1={h1}")
}

#[test]
fn test_valid_signature() {
    let verifier = PaddleVerifier::new();
    let payload = br#"{"event_type":"subscription.created"}"#;
    let secret = "pdl_ntfset_test_secret";
    let timestamp = fresh_timestamp();
    let sig = make_signature(&timestamp, payload, secret);

    assert_eq!(check(&verifier, payload, &sig, secret).unwrap(), Verified::Body);
}

#[test]
fn test_invalid_hmac() {
    let verifier = PaddleVerifier::new();
    let ts = fresh_timestamp();
    let sig = format!("ts={ts};h1=deadbeefdeadbeefdeadbeefdeadbeef");
    assert!(matches!(
        check(&verifier, b"payload", &sig, "secret"),
        Err(SignatureError::Mismatch)
    ));
}

#[test]
fn test_invalid_format_missing_ts() {
    let verifier = PaddleVerifier::new();
    let result = check(&verifier, b"payload", "h1=abc123", "secret");
    assert!(matches!(result, Err(SignatureError::InvalidFormat)));
}

#[test]
fn test_invalid_format_missing_h1() {
    let verifier = PaddleVerifier::new();
    let ts = fresh_timestamp();
    let sig = format!("ts={ts}");
    let result = check(&verifier, b"payload", &sig, "secret");
    assert!(matches!(result, Err(SignatureError::InvalidFormat)));
}

#[test]
fn test_expired_timestamp_rejected() {
    let verifier = PaddleVerifier::new();
    let old_ts = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 600)
        .to_string();
    let payload = b"payload";
    let secret = "secret";
    let sig = make_signature(&old_ts, payload, secret);
    let result = check(&verifier, payload, &sig, secret);
    assert!(matches!(result, Err(SignatureError::TimestampExpired)));
}

#[test]
fn test_empty_secret_rejected() {
    let verifier = PaddleVerifier::new();
    let ts = fresh_timestamp();
    let sig = format!("ts={ts};h1=abc123");
    let result = check(&verifier, b"payload", &sig, "");

    // #1174: the signature here is also unverifiable, so the family alone would be
    // satisfied by failing at the wrong stage.
    let Err(SignatureError::KeyMaterial(message)) = result else {
        panic!("an empty secret must be KeyMaterial; got {result:?}")
    };
    assert!(
        message.contains("must not be empty"),
        "the error must name the EMPTY SECRET as the fault; got {message:?}"
    );
}

#[test]
fn test_parse_signature_valid() {
    let (ts, h1) = parse_paddle_signature("ts=1234567890;h1=abc123def456").unwrap();
    assert_eq!(ts, "1234567890");
    assert_eq!(h1, "abc123def456");
}

#[test]
fn test_parse_signature_extra_fields_ignored() {
    // Future-proofing: extra fields should not break parsing
    let (ts, h1) = parse_paddle_signature("ts=111;h2=ignored;h1=abc").unwrap();
    assert_eq!(ts, "111");
    assert_eq!(h1, "abc");
}

#[test]
fn test_with_tolerance_u64_max_clamps_not_wraps() {
    // u64::MAX as i64 wraps to -1, making (now - ts).abs() > -1 always true (rejects
    // every timestamp).  with_tolerance must clamp to i64::MAX instead.
    let verifier = PaddleVerifier::new().with_tolerance(u64::MAX);
    let payload = br#"{"event":"test"}"#;
    let secret = "secret";
    let timestamp = fresh_timestamp();
    let sig = make_signature(&timestamp, payload, secret);

    // A fresh timestamp with an effectively-infinite tolerance must be accepted.
    assert_eq!(check(&verifier, payload, &sig, secret).unwrap(), Verified::Body);
}

#[test]
fn test_with_tolerance_large_value_clamps() {
    // Any value > i64::MAX should clamp, not panic or wrap.
    let large = (i64::MAX as u64) + 1;
    let verifier = PaddleVerifier::new().with_tolerance(large);
    let payload = b"body";
    let secret = "sec";
    let timestamp = fresh_timestamp();
    let sig = make_signature(&timestamp, payload, secret);
    assert_eq!(check(&verifier, payload, &sig, secret).unwrap(), Verified::Body);
}
