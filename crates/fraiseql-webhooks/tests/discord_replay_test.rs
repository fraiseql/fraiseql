//! SR-6: O4 — Discord verifier (like Slack) never checked timestamp freshness.
//!       Fix: the verifier rejects timestamps older than 5 minutes (300 seconds).
//!
//! Discord uses Ed25519 signatures. The signed message is `timestamp + body`.
//!
//! **Infrastructure:** none
//! **Parallelism:** safe

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use ed25519_dalek::{Signature, Signer, SigningKey};
use fraiseql_webhooks::{
    SignatureError, signature::discord::DiscordVerifier, traits::SignatureVerifier as _,
};

// ---------------------------------------------------------------------------
// Test key seeds (deterministic — avoids OsRng dependency)
// ---------------------------------------------------------------------------

const KEY_SEED_A: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0x44,
    0xda, 0x08, 0x64, 0x1e, 0xea, 0x2a, 0x4f, 0xc5, 0x38, 0xe0, 0x17, 0xd5, 0x86, 0x64, 0x6e, 0xa6,
];

const KEY_SEED_B: [u8; 32] = [
    0x4c, 0xcd, 0x08, 0x9b, 0x28, 0xff, 0x96, 0xda, 0x9d, 0xb6, 0xc3, 0x46, 0xec, 0x11, 0x4e, 0x0f,
    0x5b, 0x8a, 0x31, 0x9f, 0x35, 0xab, 0xa6, 0x24, 0xda, 0x8c, 0xf6, 0xed, 0x4d, 0x73, 0x47, 0x00,
];

/// Build a test Ed25519 signing key from a 32-byte seed and return
/// `(hex_public_key, signing_key)`.
fn ed25519_key_from_seed(seed: [u8; 32]) -> (String, SigningKey) {
    let signing_key = SigningKey::from_bytes(&seed);
    let hex_pub = hex::encode(signing_key.verifying_key().as_bytes());
    (hex_pub, signing_key)
}

/// Compute the Ed25519 signature over `timestamp + body` and return the hex-encoded result.
fn discord_sign(signing_key: &SigningKey, timestamp: u64, body: &[u8]) -> String {
    let ts_str = timestamp.to_string();
    let mut message = ts_str.as_bytes().to_vec();
    message.extend_from_slice(body);
    let signature: Signature = signing_key.sign(&message);
    hex::encode(signature.to_bytes())
}

/// Return the current Unix timestamp in seconds.
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock must be after Unix epoch")
        .as_secs()
}

// ---------------------------------------------------------------------------
// SR-6 regression tests
// ---------------------------------------------------------------------------

/// A valid Ed25519 signature with a fresh timestamp must be accepted.
#[test]
fn fresh_discord_ed25519_signature_verifies() {
    let (hex_pub_key, signing_key) = ed25519_key_from_seed(KEY_SEED_A);
    let now = now_secs();
    let body = b"interaction payload";
    let hex_sig = discord_sign(&signing_key, now, body);
    let ts_str = now.to_string();

    let verifier = DiscordVerifier::new();
    let result = verifier.verify(body, &hex_sig, &hex_pub_key, Some(&ts_str), None);

    assert!(
        result.unwrap_or(false),
        "SR-6 regression: valid fresh Discord Ed25519 signature rejected"
    );
}

/// A replayed Discord signature with a timestamp 10 minutes in the past must be rejected.
///
/// Before the O4 fix, there was no timestamp check. A captured signature could
/// be replayed at any time.
#[test]
fn stale_discord_signature_is_rejected_as_replay() {
    let (hex_pub_key, signing_key) = ed25519_key_from_seed(KEY_SEED_A);
    let past_secs = now_secs() - 600; // 10 minutes ago — outside the 5-minute window
    let body = b"interaction payload";
    let hex_sig = discord_sign(&signing_key, past_secs, body);
    let ts_str = past_secs.to_string();

    let verifier = DiscordVerifier::new();
    let result = verifier.verify(body, &hex_sig, &hex_pub_key, Some(&ts_str), None);

    assert!(
        result.is_err(),
        "SR-6 / O4 regression: stale Discord signature accepted (replay not blocked); got: {result:?}"
    );

    assert!(
        matches!(result, Err(SignatureError::TimestampExpired)),
        "O4 regression: expected TimestampExpired error, got: {result:?}"
    );
}

/// A signature from one key must be rejected when verified with a different key.
#[test]
fn discord_signature_from_different_key_is_rejected() {
    let (hex_pub_key_a, signing_key_a) = ed25519_key_from_seed(KEY_SEED_A);
    let (hex_pub_key_b, _signing_key_b) = ed25519_key_from_seed(KEY_SEED_B);

    // Sanity check: keys are actually different.
    assert_ne!(hex_pub_key_a, hex_pub_key_b);

    let now = now_secs();
    let body = b"interaction payload";
    // Sign with key A
    let hex_sig = discord_sign(&signing_key_a, now, body);
    let ts_str = now.to_string();

    // Verify against key B — must fail
    let verifier = DiscordVerifier::new();
    let result = verifier.verify(body, &hex_sig, &hex_pub_key_b, Some(&ts_str), None);

    assert!(
        result.is_err() || !result.unwrap_or(true),
        "SR-6 regression: Discord signature from key A verified against key B"
    );
}

/// A missing timestamp must return a `MissingTimestamp` error.
#[test]
fn discord_verification_without_timestamp_returns_error() {
    let (hex_pub_key, _signing_key) = ed25519_key_from_seed(KEY_SEED_A);
    let verifier = DiscordVerifier::new();
    let result = verifier.verify(b"payload", "deaddead", &hex_pub_key, None, None);

    assert!(
        matches!(result, Err(SignatureError::MissingTimestamp)),
        "O4 regression: missing timestamp must return MissingTimestamp error, got: {result:?}"
    );
}

/// A Discord signature computed over `timestamp + different_body` must be rejected
/// when the actual body is different. This ensures the body is included in verification.
#[test]
fn discord_signature_over_wrong_body_is_rejected() {
    let (hex_pub_key, signing_key) = ed25519_key_from_seed(KEY_SEED_A);
    let now = now_secs();
    let original_body = b"original interaction";
    let tampered_body = b"tampered interaction";

    // Sign the original body
    let hex_sig = discord_sign(&signing_key, now, original_body);
    let ts_str = now.to_string();

    // Verify against tampered body — must fail
    let verifier = DiscordVerifier::new();
    let result = verifier.verify(tampered_body, &hex_sig, &hex_pub_key, Some(&ts_str), None);

    assert!(
        result.is_err() || !result.unwrap_or(true),
        "SR-6 regression: Discord signature over original body accepted for tampered body"
    );
}
