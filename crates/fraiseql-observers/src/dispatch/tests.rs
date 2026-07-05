//! Tests for the shared dispatch policy and retry driver.
#![allow(clippy::unwrap_used)] // Reason: test code — unwrap on known-valid JSON literals

use std::{
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

use super::*;
use crate::config::BackoffStrategy;

/// A policy with zero backoff so retry tests run without real delay.
fn zero_delay_policy(max_attempts: u32) -> DispatchPolicy {
    DispatchPolicy::new(
        RetryConfig {
            max_attempts,
            initial_delay_ms: 0,
            max_delay_ms: 0,
            backoff_strategy: BackoffStrategy::Fixed,
        },
        FailurePolicy::Dlq,
    )
}

#[tokio::test]
async fn retries_transient_failure_until_success() {
    // Fails twice (transient) then succeeds on the third attempt → 3 invocations.
    let calls = AtomicU32::new(0);
    let result: Result<u32, &str> = run_with_retry(
        &zero_delay_policy(3),
        |_error| true, // every error is transient
        |_error| None, // no backoff-floor hint
        |n| {
            calls.fetch_add(1, Ordering::SeqCst);
            async move { if n < 3 { Err("transient boom") } else { Ok(n) } }
        },
    )
    .await;

    assert_eq!(result, Ok(3));
    assert_eq!(calls.load(Ordering::SeqCst), 3, "attempt invoked exactly 3 times");
}

#[tokio::test]
async fn gives_up_after_max_attempts() {
    let calls = AtomicU32::new(0);
    let result: Result<u32, &str> = run_with_retry(
        &zero_delay_policy(3),
        |_error| true,
        |_error| None,
        |_n| {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Err::<u32, &str>("always fails") }
        },
    )
    .await;

    assert_eq!(result, Err("always fails"));
    assert_eq!(calls.load(Ordering::SeqCst), 3, "stopped after max_attempts");
}

#[tokio::test]
async fn permanent_error_is_not_retried() {
    let calls = AtomicU32::new(0);
    let result: Result<u32, &str> = run_with_retry(
        &zero_delay_policy(5),
        |_error| false, // permanent
        |_error| None,
        |_n| {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Err::<u32, &str>("permanent") }
        },
    )
    .await;

    assert_eq!(result, Err("permanent"));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "permanent error is not retried");
}

#[tokio::test(start_paused = true)]
async fn honors_the_error_supplied_backoff_floor() {
    // Greylisting: a zero-delay policy would retry in seconds, but a 5-minute error
    // hint raises the floor, so the retry waits the mail-appropriate delay (virtual
    // time under the paused clock keeps this instant and deterministic).
    let start = tokio::time::Instant::now();
    let result: Result<u32, &str> = run_with_retry(
        &zero_delay_policy(2),
        |_error| true,
        |_error| Some(Duration::from_secs(300)),
        |n| async move { if n < 2 { Err("greylisted") } else { Ok(n) } },
    )
    .await;
    assert_eq!(result, Ok(2));
    assert!(
        start.elapsed() >= Duration::from_secs(300),
        "the retry waited the error's mail-appropriate floor, not the policy's zero delay"
    );
}

#[test]
fn after_transient_failure_gives_up_at_max() {
    let policy = zero_delay_policy(3);
    assert!(matches!(policy.after_transient_failure(1), RetryDecision::Retry(_)));
    assert!(matches!(policy.after_transient_failure(2), RetryDecision::Retry(_)));
    assert_eq!(policy.after_transient_failure(3), RetryDecision::GiveUp);
}

#[test]
fn backoff_delay_matches_configured_strategy() {
    // Fixed strategy → always initial_delay, exercising RetryConfig::backoff_delay,
    // the value ObserverExecutor::calculate_backoff now delegates to.
    let config = RetryConfig {
        max_attempts:     3,
        initial_delay_ms: 50,
        max_delay_ms:     1000,
        backoff_strategy: BackoffStrategy::Fixed,
    };
    assert_eq!(config.backoff_delay(1), Duration::from_millis(50));
    assert_eq!(config.backoff_delay(9), Duration::from_millis(50));
}

// ── Idempotency token derivation ────────────────────────────────────────────────

#[test]
fn dispatch_source_label_is_stable() {
    // The label feeds the idempotency hash; it must be a stable string, decoupled
    // from `Debug`, so a token stays constant across refactors of the enum.
    assert_eq!(DispatchSource::AfterMutation.label(), "after:mutation");
    assert_eq!(DispatchSource::AfterIngest.label(), "after:ingest");
}

#[test]
fn idempotency_token_is_deterministic() {
    let payload = serde_json::json!({ "id": 42, "amount_cents": 1000 });
    let a = derive_idempotency_token(
        None,
        DispatchSource::AfterMutation,
        "syncInvoice",
        "after:mutation:Invoice:create",
        &payload,
    );
    let b = derive_idempotency_token(
        None,
        DispatchSource::AfterMutation,
        "syncInvoice",
        "after:mutation:Invoice:create",
        &payload,
    );
    assert_eq!(a, b, "same inputs must yield the same token (stable across retries and resume)");
}

#[test]
fn idempotency_token_is_email_safe_hex() {
    // The token doubles as a VERP send-id (`bounces+<token>@domain`), so it must be
    // lowercase hex only and short enough for a 64-char local part — in both modes.
    let payload = serde_json::json!({ "id": 7 });
    for key in [None, Some(b"server-secret".as_slice())] {
        let token = derive_idempotency_token(
            key,
            DispatchSource::AfterMutation,
            "f",
            "after:mutation:X:create",
            &payload,
        );
        assert_eq!(token.len(), 32, "128-bit truncated digest → 32 hex chars");
        assert!(
            token.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "token must be lowercase hex only, got {token:?}"
        );
    }
}

#[test]
fn idempotency_token_distinct_per_function() {
    let payload = serde_json::json!({ "id": 1 });
    let a = derive_idempotency_token(
        None,
        DispatchSource::AfterMutation,
        "sendFollowUp",
        "after:mutation:Deal:update",
        &payload,
    );
    let b = derive_idempotency_token(
        None,
        DispatchSource::AfterMutation,
        "syncQonto",
        "after:mutation:Deal:update",
        &payload,
    );
    assert_ne!(a, b, "distinct functions on the same event are distinct logical operations");
}

#[test]
fn idempotency_token_distinct_per_trigger() {
    let payload = serde_json::json!({ "id": 1 });
    let a = derive_idempotency_token(
        None,
        DispatchSource::AfterMutation,
        "f",
        "after:mutation:Deal:create",
        &payload,
    );
    let b = derive_idempotency_token(
        None,
        DispatchSource::AfterMutation,
        "f",
        "after:mutation:Deal:update",
        &payload,
    );
    assert_ne!(a, b);
}

#[test]
fn idempotency_token_distinct_per_payload() {
    let a = derive_idempotency_token(
        None,
        DispatchSource::AfterMutation,
        "f",
        "after:mutation:Deal:update",
        &serde_json::json!({ "id": 1 }),
    );
    let b = derive_idempotency_token(
        None,
        DispatchSource::AfterMutation,
        "f",
        "after:mutation:Deal:update",
        &serde_json::json!({ "id": 2 }),
    );
    assert_ne!(a, b, "distinct entities are distinct logical operations");
}

#[test]
fn idempotency_token_distinct_per_source() {
    let payload = serde_json::json!({ "id": 1 });
    let a = derive_idempotency_token(None, DispatchSource::AfterMutation, "f", "t", &payload);
    let b = derive_idempotency_token(None, DispatchSource::AfterIngest, "f", "t", &payload);
    assert_ne!(a, b, "the source is part of the dispatch identity");
}

#[test]
fn idempotency_token_ignores_object_key_order() {
    // Two payloads that differ only in textual key order must hash identically:
    // the token is canonical (serde_json::Value sorts object keys), which is what
    // makes it stable across a resume that re-serialises the payload.
    let a: serde_json::Value = serde_json::from_str(r#"{"a":1,"b":2}"#).unwrap();
    let b: serde_json::Value = serde_json::from_str(r#"{"b":2,"a":1}"#).unwrap();
    let ta = derive_idempotency_token(None, DispatchSource::AfterMutation, "f", "t", &a);
    let tb = derive_idempotency_token(None, DispatchSource::AfterMutation, "f", "t", &b);
    assert_eq!(ta, tb, "canonical payload → order-independent token");
}

// ── Keyed (HMAC) mode — the unforgeable VERP send-id ─────────────────────────────

#[test]
fn keyed_token_differs_from_unkeyed() {
    // The keyed (HMAC) token must not equal the plain digest of the same identity —
    // otherwise the secret adds nothing and the send-id stays forgeable.
    let payload = serde_json::json!({ "id": 1 });
    let plain = derive_idempotency_token(None, DispatchSource::AfterMutation, "f", "t", &payload);
    let keyed = derive_idempotency_token(
        Some(b"server-secret"),
        DispatchSource::AfterMutation,
        "f",
        "t",
        &payload,
    );
    assert_ne!(plain, keyed, "the HMAC key must change the token");
}

#[test]
fn keyed_token_is_deterministic() {
    // Same key + same identity → same token (still resume-stable, retry-stable).
    let payload = serde_json::json!({ "id": 1 });
    let a = derive_idempotency_token(Some(b"k"), DispatchSource::AfterMutation, "f", "t", &payload);
    let b = derive_idempotency_token(Some(b"k"), DispatchSource::AfterMutation, "f", "t", &payload);
    assert_eq!(a, b, "keyed derivation is deterministic");
}

#[test]
fn keyed_token_differs_by_key() {
    // A different secret yields a different token: an attacker without the secret
    // cannot mint a valid send-id even knowing the identity fields.
    let payload = serde_json::json!({ "id": 1 });
    let a = derive_idempotency_token(
        Some(b"secret-a"),
        DispatchSource::AfterMutation,
        "f",
        "t",
        &payload,
    );
    let b = derive_idempotency_token(
        Some(b"secret-b"),
        DispatchSource::AfterMutation,
        "f",
        "t",
        &payload,
    );
    assert_ne!(a, b, "the token binds to the secret");
}

#[test]
fn subkey_is_deterministic_and_root_dependent() {
    // The subkey is a stable function of the root (so tokens survive restart), and
    // domain-separated so a different root yields a different subkey.
    assert_eq!(derive_idempotency_subkey(b"root"), derive_idempotency_subkey(b"root"));
    assert_ne!(derive_idempotency_subkey(b"root-a"), derive_idempotency_subkey(b"root-b"));
    // Domain separation: the subkey is not the raw root truncated/padded.
    assert_ne!(&derive_idempotency_subkey(b"root")[..], b"root");
}

// ── Suppression address hashing (GDPR: never store the raw address) ───────────────

#[test]
fn address_hash_key_is_domain_separated_from_the_send_id_subkey() {
    // The two subkeys derive from the same root but must be independent — one must
    // not be recoverable from the other.
    assert_ne!(derive_address_hash_key(b"root"), derive_idempotency_subkey(b"root"));
    // Deterministic + root-dependent, like the send-id subkey.
    assert_eq!(derive_address_hash_key(b"root"), derive_address_hash_key(b"root"));
    assert_ne!(derive_address_hash_key(b"root-a"), derive_address_hash_key(b"root-b"));
}

#[test]
fn address_hash_is_deterministic_case_and_whitespace_insensitive() {
    let key = derive_address_hash_key(b"root");
    let canonical = hash_address(&key, "bob@example.com");
    // Casing and surrounding whitespace must not split one recipient into distinct
    // suppression entries.
    assert_eq!(hash_address(&key, "Bob@Example.COM"), canonical);
    assert_eq!(hash_address(&key, "  bob@example.com  "), canonical);
    // A full HMAC-SHA256, hex-encoded → 64 chars, lowercase hex.
    assert_eq!(canonical.len(), 64);
    assert!(canonical.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
}

#[test]
fn address_hash_binds_to_the_key() {
    // Without the key an attacker cannot recompute the stored hash from the address,
    // and a different address hashes differently under the same key.
    let key_a = derive_address_hash_key(b"root-a");
    let key_b = derive_address_hash_key(b"root-b");
    assert_ne!(hash_address(&key_a, "bob@example.com"), hash_address(&key_b, "bob@example.com"));
    assert_ne!(hash_address(&key_a, "bob@example.com"), hash_address(&key_a, "eve@example.com"));
}
