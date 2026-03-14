//! SR-5: O4 — Slack verifier never checked timestamp freshness.
//!       A captured signature could be replayed indefinitely.
//!       Fix: the verifier rejects timestamps older than 5 minutes (300 seconds).
//!
//! Because `SlackVerifier` uses `SystemTime::now()` directly (no clock injection),
//! these tests use real wall-clock timestamps: a fresh timestamp to verify acceptance
//! and a timestamp 10 minutes in the past to verify replay rejection.
//!
//! **Infrastructure:** none
//! **Parallelism:** safe

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use fraiseql_webhooks::{
    SignatureError, signature::slack::SlackVerifier, traits::SignatureVerifier as _,
};
use hmac::{Hmac, Mac};
use sha2::Sha256;

const SIGNING_SECRET: &str = "8f742231b10e8888abcd99yyyzzz85a5";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Compute `v0=<hex>` over the Slack canonical message: `v0:<timestamp>:<body>`.
fn make_slack_signature(secret: &str, timestamp: u64, body: &[u8]) -> String {
    let base_str = format!("v0:{}:{}", timestamp, String::from_utf8_lossy(body));
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(base_str.as_bytes());
    format!("v0={}", hex::encode(mac.finalize().into_bytes()))
}

/// Return the current Unix timestamp in seconds.
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock must be after Unix epoch")
        .as_secs()
}

// ---------------------------------------------------------------------------
// SR-5 regression tests
// ---------------------------------------------------------------------------

/// A valid Slack signature with a fresh timestamp must be accepted.
///
/// The timestamp is set to `now`, which is within the 5-minute replay window.
#[test]
fn fresh_slack_signature_verifies() {
    let now = now_secs();
    let body = b"payload=test_body";
    let sig = make_slack_signature(SIGNING_SECRET, now, body);
    let ts_str = now.to_string();

    let verifier = SlackVerifier::new();
    let result = verifier.verify(body, &sig, SIGNING_SECRET, Some(&ts_str), None);

    assert!(result.unwrap_or(false), "SR-5 regression: valid fresh Slack signature rejected");
}

/// A replayed Slack signature with a timestamp 10 minutes in the past must be rejected.
///
/// Before the O4 fix, there was no timestamp check. A captured signature could
/// be replayed at any time.
#[test]
fn stale_slack_signature_is_rejected_as_replay() {
    let past_secs = now_secs() - 600; // 10 minutes ago — well outside the 5-minute window
    let body = b"payload=test_body";
    let sig = make_slack_signature(SIGNING_SECRET, past_secs, body);
    let ts_str = past_secs.to_string();

    let verifier = SlackVerifier::new();
    let result = verifier.verify(body, &sig, SIGNING_SECRET, Some(&ts_str), None);

    assert!(
        result.is_err(),
        "SR-5 / O4 regression: stale Slack signature accepted (replay not blocked); got: {result:?}"
    );

    // Verify the error is specifically a timestamp-expiry error, not a signature error.
    assert!(
        matches!(result, Err(SignatureError::TimestampExpired)),
        "O4 regression: expected TimestampExpired error, got: {result:?}"
    );
}

/// A signature with a missing timestamp must return a clear error.
/// Without a timestamp, replay protection is impossible.
#[test]
fn slack_signature_without_timestamp_returns_missing_timestamp_error() {
    let body = b"payload=test_body";
    let verifier = SlackVerifier::new();
    let result = verifier.verify(body, "v0=invalidsig", SIGNING_SECRET, None, None);

    assert!(
        matches!(result, Err(SignatureError::MissingTimestamp)),
        "O4 regression: missing timestamp must return MissingTimestamp error, got: {result:?}"
    );
}

/// A Slack signature using a wrong secret must be rejected (signature mismatch).
/// This verifies the HMAC verification is also working, not just timestamp checks.
#[test]
fn slack_signature_with_wrong_secret_is_rejected() {
    let now = now_secs();
    let body = b"payload=test_body";
    // Sign with correct secret, verify with wrong one
    let sig = make_slack_signature("correct_secret", now, body);
    let ts_str = now.to_string();

    let verifier = SlackVerifier::new();
    let result = verifier.verify(body, &sig, "wrong_secret", Some(&ts_str), None);

    assert!(
        result.is_err() || !result.unwrap_or(true),
        "SR-5 regression: Slack signature verified with wrong secret"
    );
}

/// Custom tolerance: a signature that would be rejected by the 300-second default
/// must be accepted by a verifier configured with a longer tolerance.
#[test]
fn slack_verifier_custom_tolerance_accepts_older_signatures() {
    let past_secs = now_secs() - 400; // 400 s ago — outside 300 s default, inside 600 s custom
    let body = b"payload=custom_tolerance";
    let sig = make_slack_signature(SIGNING_SECRET, past_secs, body);
    let ts_str = past_secs.to_string();

    let verifier = SlackVerifier::new().with_tolerance(600); // 10-minute window
    let result = verifier.verify(body, &sig, SIGNING_SECRET, Some(&ts_str), None);

    assert!(
        result.unwrap_or(false),
        "Custom tolerance: signature 400 s old should be accepted within 600 s window"
    );
}
