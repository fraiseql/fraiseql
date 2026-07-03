//! Tests for the shared dispatch policy and retry driver.

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
