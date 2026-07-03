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

use uuid::Uuid;

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

/// Which trigger subsystem produced a dead-lettered function dispatch.
///
/// Recorded on every [`FunctionDispatchRecord`] so a single dead-letter queue can
/// hold — and be filtered by — failures from more than one dispatch source. The
/// enum is `#[non_exhaustive]`: the inbound-ingestion source (`after:ingest`)
/// lands in a later phase of this train.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DispatchSource {
    /// An `after:mutation` function trigger.
    AfterMutation,
}

/// A function-trigger dispatch that exhausted its retries (or failed
/// permanently) and was routed to the dead-letter queue.
///
/// This is the function-dispatch analogue of the observer
/// [`DlqItem`](crate::traits::DlqItem): where an observer DLQ entry carries an
/// [`EntityEvent`](crate::event::EntityEvent) + [`ActionConfig`](crate::config::ActionConfig),
/// a function DLQ entry carries the module name, the trigger type, and the event
/// payload as opaque JSON (the observer crate does not depend on
/// `fraiseql-functions`). Both live in the same store, discriminated by
/// [`source`](Self::source), so money- and send-path work is inspectable and
/// replayable rather than silently lost.
// Reason: the `payload` field is a `serde_json::Value`, which is not `Eq`
// (floats), so the nursery `derive_partial_eq_without_eq` suggestion cannot hold.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDispatchRecord {
    /// Unique identifier for this dead-letter entry.
    pub id:            Uuid,
    /// Which trigger subsystem produced the failed dispatch.
    pub source:        DispatchSource,
    /// Name of the function whose dispatch failed.
    pub function_name: String,
    /// The trigger type string, e.g. `after:mutation:onUserCreated`.
    pub trigger_type:  String,
    /// The event payload the function was dispatched with (opaque JSON), kept for
    /// operator inspection and replay.
    pub payload:       serde_json::Value,
    /// The final error message from the exhausted or permanently-failed dispatch.
    pub error_message: String,
    /// How many attempts were made before the dispatch was dead-lettered.
    pub attempts:      u32,
}

impl FunctionDispatchRecord {
    /// Build a dead-letter record, minting a fresh [`id`](Self::id).
    #[must_use]
    pub fn new(
        source: DispatchSource,
        function_name: impl Into<String>,
        trigger_type: impl Into<String>,
        payload: serde_json::Value,
        error_message: impl Into<String>,
        attempts: u32,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            source,
            function_name: function_name.into(),
            trigger_type: trigger_type.into(),
            payload,
            error_message: error_message.into(),
            attempts,
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
mod tests;
