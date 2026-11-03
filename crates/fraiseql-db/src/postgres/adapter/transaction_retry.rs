//! Re-running a mutation's transaction that PostgreSQL aborted as a deadlock victim or a
//! serialization failure (#1879).

use std::{fmt, sync::Arc, time::Duration};

use fraiseql_error::{FraiseQLError, Result};

/// The longest a policy may sleep between two attempts, in milliseconds.
///
/// The connection is held across the sleep, so a typo must not park it for minutes.
pub const MAX_BACKOFF_MS: u64 = 5_000;

/// What a [`TransactionRetry`] observer is told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryEvent<'a> {
    /// The transaction was rolled back and will run again after `backoff`.
    Retrying {
        /// The function whose call (or whose transaction's `COMMIT`) failed.
        function:  &'a str,
        /// `40P01` or `40001`.
        sql_state: &'a str,
        /// The attempt that failed, from 1.
        attempt:   u32,
        /// How long the next attempt waits.
        backoff:   Duration,
    },
    /// The last attempt failed too: its error is returned.
    Exhausted {
        /// The function whose call (or whose transaction's `COMMIT`) failed.
        function:  &'a str,
        /// `40P01` or `40001`.
        sql_state: &'a str,
        /// How many attempts ran.
        attempts:  u32,
    },
}

/// Observes a [`TransactionRetry`]'s decisions, for metrics.
pub type RetryObserver = Arc<dyn Fn(RetryEvent<'_>) + Send + Sync>;

/// How often a mutation's transaction runs when PostgreSQL aborts it with `40P01`
/// (deadlock detected) or `40001` (serialization failure).
///
/// Off by default: one attempt. A retried function runs its body again, so an effect
/// outside the transaction can repeat.
#[derive(Clone)]
pub struct TransactionRetry {
    max_attempts: u32,
    base:         Duration,
    max:          Duration,
    observer:     Option<RetryObserver>,
}

impl TransactionRetry {
    /// One attempt: an aborted transaction's error is returned, as without a policy.
    #[must_use]
    pub const fn off() -> Self {
        Self {
            max_attempts: 1,
            base:         Duration::ZERO,
            max:          Duration::ZERO,
            observer:     None,
        }
    }

    /// Up to `max_attempts` attempts, the `n`th retry waiting a random duration up to
    /// `base_ms · 2ⁿ⁻¹`, capped at `max_ms`.
    ///
    /// # Errors
    ///
    /// `FraiseQLError::Configuration` naming the bound broken: `max_attempts` is 0,
    /// `base_ms` exceeds `max_ms`, or `max_ms` exceeds [`MAX_BACKOFF_MS`].
    pub const fn new(max_attempts: u32, base_ms: u64, max_ms: u64) -> Result<Self> {
        Ok(Self {
            max_attempts,
            base: Duration::from_millis(base_ms),
            max: Duration::from_millis(max_ms),
            observer: None,
        })
    }

    /// Tell `observer` about each retry and each exhaustion.
    #[must_use]
    pub fn with_observer(mut self, observer: RetryObserver) -> Self {
        self.observer = Some(observer);
        self
    }

    /// How many attempts a transaction gets, the first included.
    #[must_use]
    pub const fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    /// The wait before retry number `retry` (from 1), drawn from `entropy`.
    #[allow(dead_code)] // Reason: the RED stub's adapter never retries
    pub(crate) fn backoff(&self, _retry: u32, entropy: u64) -> Duration {
        let jitter = entropy % (u64::try_from(self.max.as_millis()).unwrap_or(u64::MAX) + 1);
        self.max + Duration::from_millis(jitter)
    }

    #[allow(dead_code)] // Reason: the RED stub's adapter never retries
    pub(crate) fn observe(&self, event: RetryEvent<'_>) {
        if let Some(observer) = &self.observer {
            observer(event);
        }
    }
}

impl Default for TransactionRetry {
    fn default() -> Self {
        Self::off()
    }
}

impl fmt::Debug for TransactionRetry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TransactionRetry")
            .field("max_attempts", &self.max_attempts)
            .field("base", &self.base)
            .field("max", &self.max)
            .field("observer", &self.observer.is_some())
            .finish()
    }
}

#[allow(dead_code)] // Reason: the RED stub names the error it will return
fn refused(message: &str) -> FraiseQLError {
    FraiseQLError::Configuration {
        message: format!("transaction_retry: {message}"),
    }
}

#[cfg(test)]
#[path = "transaction_retry_tests.rs"]
mod tests;
