//! Shared dispatch policy and retry driver for durable event dispatch.
//!
//! Both the observer action executor and the server's `after:mutation` function
//! dispatcher need the same behaviour: retry *transient* failures with backoff,
//! give up after a bounded number of attempts, and route the exhausted work to a
//! dead-letter queue. This module holds the reusable pieces so neither subsystem
//! reimplements them:
//!
//! - [`DispatchPolicy`] bundles a [`RetryConfig`] with a [`FailurePolicy`].
//! - [`run_with_retry`] is a runtime-agnostic retry loop driven by that policy.
//!
//! Backoff timing itself lives on [`RetryConfig::backoff_delay`], the single
//! source of truth that the observer executor also delegates to, so retries age
//! identically across both subsystems.

use std::future::Future;

use crate::config::{FailurePolicy, RetryConfig};

/// A reusable dispatch policy: how many times to retry, how long to back off, and
/// what to do once retries are exhausted.
///
/// Shared by the observer subsystem and the function-trigger dispatcher so that
/// "durable dispatch" means the same thing in both.
#[derive(Debug, Clone)]
pub struct DispatchPolicy {
    /// Retry/backoff configuration.
    pub retry:   RetryConfig,
    /// What to do when dispatch fails permanently or exhausts its retries.
    pub failure: FailurePolicy,
}

impl DispatchPolicy {
    /// Construct a policy from its retry and failure parts.
    #[must_use]
    pub const fn new(retry: RetryConfig, failure: FailurePolicy) -> Self {
        Self { retry, failure }
    }

    /// Decide what to do after a *transient* failure on the 1-based `attempt`.
    ///
    /// Returns [`RetryDecision::Retry`] with the backoff delay while attempts
    /// remain, or [`RetryDecision::GiveUp`] once `attempt` reaches
    /// `retry.max_attempts` — at which point the caller applies [`Self::failure`]
    /// (e.g. dead-letters the work).
    #[must_use]
    pub fn after_transient_failure(&self, attempt: u32) -> RetryDecision {
        if attempt >= self.retry.max_attempts {
            RetryDecision::GiveUp
        } else {
            RetryDecision::Retry(self.retry.backoff_delay(attempt))
        }
    }
}

/// What [`DispatchPolicy::after_transient_failure`] decided to do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDecision {
    /// Wait the given delay, then try again.
    Retry(std::time::Duration),
    /// Stop retrying; the caller should apply its failure policy (e.g. DLQ).
    GiveUp,
}

/// Run `attempt` under `policy`, retrying transient failures with backoff.
///
/// `attempt` is invoked with the 1-based attempt number and produces the dispatch
/// result. `is_transient` classifies an error as retryable (transient) or
/// permanent; a permanent error aborts immediately without consuming further
/// attempts. On success the value is returned; otherwise the final error — from a
/// permanent failure or the last exhausted attempt — is returned so the caller
/// can dead-letter it.
///
/// The loop is runtime-agnostic: backoff waits use `tokio::time::sleep`, so a
/// zero backoff (`initial_delay_ms = 0`) runs with no real delay, which keeps the
/// unit tests instant.
///
/// # Errors
///
/// Returns the error `E` from the last attempt: either a permanent error (one
/// `is_transient` rejected) that aborted immediately, or the error from the final
/// attempt once `policy.retry.max_attempts` transient failures were exhausted.
pub async fn run_with_retry<T, E, F, Fut>(
    policy: &DispatchPolicy,
    is_transient: impl Fn(&E) -> bool,
    mut attempt: F,
) -> Result<T, E>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut n = 0;
    loop {
        n += 1;
        match attempt(n).await {
            Ok(value) => return Ok(value),
            Err(error) => {
                if !is_transient(&error) {
                    return Err(error);
                }
                match policy.after_transient_failure(n) {
                    RetryDecision::Retry(delay) => tokio::time::sleep(delay).await,
                    RetryDecision::GiveUp => return Err(error),
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
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
                async move {
                    if n < 3 {
                        Err("transient boom")
                    } else {
                        Ok(n)
                    }
                }
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
}
