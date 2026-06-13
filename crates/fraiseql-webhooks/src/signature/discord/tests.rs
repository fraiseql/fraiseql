#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use ed25519_dalek::{Signer, SigningKey};

use super::*;

/// Deterministic test seed — avoids `OsRng` in unit tests for reproducibility.
const TEST_KEY_SEED: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0x44,
    0xda, 0x08, 0x64, 0x1e, 0xea, 0x2a, 0x4f, 0xc5, 0x38, 0xe0, 0x17, 0xd5, 0x86, 0x64, 0x6e, 0xa6,
];

fn fresh_timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        .to_string()
}

/// Build a signing key from a fixed seed, sign `timestamp + payload`, and return
/// `(hex_public_key, hex_signature)`.
fn make_valid_discord_signature(timestamp: &str, payload: &[u8]) -> (String, String) {
    let signing_key = SigningKey::from_bytes(&TEST_KEY_SEED);
    let verifying_key = signing_key.verifying_key();

    let mut message = timestamp.as_bytes().to_vec();
    message.extend_from_slice(payload);

    let signature = signing_key.sign(&message);
    (hex::encode(verifying_key.as_bytes()), hex::encode(signature.to_bytes()))
}

#[test]
fn test_valid_signature_accepted() {
    let verifier = DiscordVerifier::new();
    let ts = fresh_timestamp();
    let payload = br#"{"type":1}"#;
    let (public_key_hex, sig_hex) = make_valid_discord_signature(&ts, payload);

    let result = verifier.verify(payload, &sig_hex, &public_key_hex, Some(&ts), None);
    assert!(
        matches!(result, Ok(true)),
        "valid Ed25519 signature should be accepted; got: {result:?}"
    );
}

#[test]
fn test_tampered_payload_rejected() {
    let verifier = DiscordVerifier::new();
    let ts = fresh_timestamp();
    let (public_key_hex, sig_hex) = make_valid_discord_signature(&ts, br#"{"type":1}"#);

    // Different payload — signature is no longer valid.
    let result = verifier.verify(b"tampered", &sig_hex, &public_key_hex, Some(&ts), None);
    assert!(
        matches!(result, Ok(false)),
        "tampered payload should be rejected; got: {result:?}"
    );
}

#[test]
fn test_missing_timestamp() {
    let verifier = DiscordVerifier::new();
    let result = verifier.verify(b"test", "abc", "deadbeef", None, None);
    assert!(matches!(result, Err(SignatureError::MissingTimestamp)));
}

#[test]
fn test_expired_timestamp_rejected() {
    let verifier = DiscordVerifier::new();
    let old_ts = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 600)
        .to_string();
    // Even with a valid signature format, an old timestamp should be rejected.
    let result = verifier.verify(b"payload", "deadbeef", "deadbeef", Some(&old_ts), None);
    assert!(matches!(result, Err(SignatureError::TimestampExpired)));
}

#[test]
fn test_invalid_public_key_hex() {
    let verifier = DiscordVerifier::new();
    let ts = fresh_timestamp();
    let result = verifier.verify(b"test", "abc123", "not-hex!", Some(&ts), None);
    assert!(matches!(result, Err(SignatureError::Crypto(_))));
}

#[test]
fn test_with_tolerance_large_value_does_not_wrap() {
    // The tolerance is stored verbatim as a u64 (no wrap at storage); the shared
    // `check_timestamp_freshness` saturates it to i64::MAX at comparison time.
    let verifier = DiscordVerifier::new().with_tolerance(u64::MAX);
    assert_eq!(verifier.tolerance_secs, u64::MAX);
}
