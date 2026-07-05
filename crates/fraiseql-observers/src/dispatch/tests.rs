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
        |_n| {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Err::<u32, &str>("permanent") }
        },
    )
    .await;

    assert_eq!(result, Err("permanent"));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "permanent error is not retried");
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
        DispatchSource::AfterMutation,
        "syncInvoice",
        "after:mutation:Invoice:create",
        &payload,
    );
    let b = derive_idempotency_token(
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
    // lowercase hex only and short enough for a 64-char local part.
    let payload = serde_json::json!({ "id": 7 });
    let token = derive_idempotency_token(
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

#[test]
fn idempotency_token_distinct_per_function() {
    let payload = serde_json::json!({ "id": 1 });
    let a = derive_idempotency_token(
        DispatchSource::AfterMutation,
        "sendFollowUp",
        "after:mutation:Deal:update",
        &payload,
    );
    let b = derive_idempotency_token(
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
        DispatchSource::AfterMutation,
        "f",
        "after:mutation:Deal:create",
        &payload,
    );
    let b = derive_idempotency_token(
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
        DispatchSource::AfterMutation,
        "f",
        "after:mutation:Deal:update",
        &serde_json::json!({ "id": 1 }),
    );
    let b = derive_idempotency_token(
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
    let a = derive_idempotency_token(DispatchSource::AfterMutation, "f", "t", &payload);
    let b = derive_idempotency_token(DispatchSource::AfterIngest, "f", "t", &payload);
    assert_ne!(a, b, "the source is part of the dispatch identity");
}

#[test]
fn idempotency_token_ignores_object_key_order() {
    // Two payloads that differ only in textual key order must hash identically:
    // the token is canonical (serde_json::Value sorts object keys), which is what
    // makes it stable across a resume that re-serialises the payload.
    let a: serde_json::Value = serde_json::from_str(r#"{"a":1,"b":2}"#).unwrap();
    let b: serde_json::Value = serde_json::from_str(r#"{"b":2,"a":1}"#).unwrap();
    let ta = derive_idempotency_token(DispatchSource::AfterMutation, "f", "t", &a);
    let tb = derive_idempotency_token(DispatchSource::AfterMutation, "f", "t", &b);
    assert_eq!(ta, tb, "canonical payload → order-independent token");
}
