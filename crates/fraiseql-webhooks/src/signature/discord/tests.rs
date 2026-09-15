#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::panic)] // Reason: test module — a `let ... else` that cannot bind the expected error variant must fail loudly and say what it got (#1174)

use std::collections::BTreeMap;

use ed25519_dalek::{Signer, SigningKey};

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
    headers.insert("X-Signature-Ed25519".to_ascii_lowercase(), signature.to_string());
    if let Some(timestamp) = timestamp {
        headers.insert("X-Signature-Timestamp".to_ascii_lowercase(), timestamp.to_string());
    }
    verifier.verify(&InboundRequest::new(&headers, payload, None), secret)
}

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

    let result = check(&verifier, payload, &sig_hex, &public_key_hex, Some(&ts));
    assert!(
        matches!(result, Ok(Verified::Body)),
        "valid Ed25519 signature should be accepted; got: {result:?}"
    );
}

#[test]
fn test_tampered_payload_rejected() {
    let verifier = DiscordVerifier::new();
    let ts = fresh_timestamp();
    let (public_key_hex, sig_hex) = make_valid_discord_signature(&ts, br#"{"type":1}"#);

    // Different payload — signature is no longer valid.
    let result = check(&verifier, b"tampered", &sig_hex, &public_key_hex, Some(&ts));
    assert!(
        matches!(result, Err(SignatureError::Mismatch)),
        "tampered payload should be rejected; got: {result:?}"
    );
}

#[test]
fn test_missing_timestamp() {
    let verifier = DiscordVerifier::new();
    let result = check(&verifier, b"test", "abc", "deadbeef", None);
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
    let result = check(&verifier, b"payload", "deadbeef", "deadbeef", Some(&old_ts));
    assert!(matches!(result, Err(SignatureError::TimestampExpired)));
}

#[test]
fn test_invalid_public_key_hex() {
    let verifier = DiscordVerifier::new();
    let ts = fresh_timestamp();
    let result = check(&verifier, b"test", "abc123", "not-hex!", Some(&ts));

    // #1174: assert the message, not just the family — the key and the signature are
    // both hex here, and `KeyMaterial(_)` alone cannot say which one failed to decode.
    let Err(SignatureError::KeyMaterial(message)) = result else {
        panic!("an undecodable public key must be KeyMaterial; got {result:?}")
    };
    assert!(
        message.contains("public key"),
        "the error must name the KEY as the undecodable input; got {message:?}"
    );
}

/// #1045: the counterpart to `test_invalid_public_key_hex`, and the reason
/// `KeyMaterial` had to be split out of the old `Crypto` variant.
///
/// Both inputs fail to decode as hex, but the *key* is the server's and the
/// *signature* is the sender's, so they must not share an error class: mapping
/// them alike is what let an unauthenticated caller choose between a 401 and a
/// 5xx by editing one header.
#[test]
fn an_unparseable_signature_is_the_senders_fault_not_key_material() {
    let verifier = DiscordVerifier::new();
    let ts = fresh_timestamp();
    // A real key, so verification reaches the signature decode rather than
    // stopping at the key — the failure mode that made the SendGrid twin vacuous.
    let (public_key_hex, _) = make_valid_discord_signature(&ts, br#"{"type":1}"#);

    let result = check(&verifier, b"{\"type\":1}", "zz", &public_key_hex, Some(&ts));

    assert!(
        matches!(result, Err(SignatureError::InvalidFormat)),
        "an unparseable sender signature must stay the sender's fault; got {result:?}"
    );
}

#[test]
fn test_with_tolerance_large_value_does_not_wrap() {
    // The tolerance is stored verbatim as a u64 (no wrap at storage); the shared
    // `check_timestamp_freshness` saturates it to i64::MAX at comparison time.
    let verifier = DiscordVerifier::new().with_tolerance(u64::MAX);
    assert_eq!(verifier.tolerance_secs, u64::MAX);
}
