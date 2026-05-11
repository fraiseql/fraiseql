#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::*;

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

    assert!(verifier.verify(payload, &sig, secret, None, None).unwrap());
}

#[test]
fn test_invalid_hmac() {
    let verifier = PaddleVerifier::new();
    let ts = fresh_timestamp();
    let sig = format!("ts={ts};h1=deadbeefdeadbeefdeadbeefdeadbeef");
    assert!(!verifier.verify(b"payload", &sig, "secret", None, None).unwrap());
}

#[test]
fn test_invalid_format_missing_ts() {
    let verifier = PaddleVerifier::new();
    let result = verifier.verify(b"payload", "h1=abc123", "secret", None, None);
    assert!(matches!(result, Err(SignatureError::InvalidFormat)));
}

#[test]
fn test_invalid_format_missing_h1() {
    let verifier = PaddleVerifier::new();
    let ts = fresh_timestamp();
    let sig = format!("ts={ts}");
    let result = verifier.verify(b"payload", &sig, "secret", None, None);
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
    let result = verifier.verify(payload, &sig, secret, None, None);
    assert!(matches!(result, Err(SignatureError::TimestampExpired)));
}

#[test]
fn test_empty_secret_rejected() {
    let verifier = PaddleVerifier::new();
    let ts = fresh_timestamp();
    let sig = format!("ts={ts};h1=abc123");
    let result = verifier.verify(b"payload", &sig, "", None, None);
    assert!(matches!(result, Err(SignatureError::Crypto(_))));
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
    assert!(verifier.verify(payload, &sig, secret, None, None).unwrap());
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
    assert!(verifier.verify(payload, &sig, secret, None, None).unwrap());
}
