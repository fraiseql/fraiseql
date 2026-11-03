//! Re-running a mutation's transaction that PostgreSQL aborted as a deadlock victim or a
//! serialization failure (#1879).

use std::{
    fmt,
    hash::{BuildHasher as _, RandomState},
    sync::Arc,
    time::Duration,
};

use fraiseql_error::{FraiseQLError, Result};
use serde::{Deserialize, Serialize};

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
    base_ms:      u64,
    max_ms:       u64,
    observer:     Option<RetryObserver>,
}

impl TransactionRetry {
    /// One attempt: an aborted transaction's error is returned, as without a policy.
    #[must_use]
    pub const fn off() -> Self {
        Self {
            max_attempts: 1,
            base_ms:      0,
            max_ms:       0,
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
    pub fn new(max_attempts: u32, base_ms: u64, max_ms: u64) -> Result<Self> {
        if max_attempts == 0 {
            return Err(refused(
                "max_attempts must be at least 1 (the first attempt): 1 turns retry off",
            ));
        }
        if max_ms > MAX_BACKOFF_MS {
            return Err(refused(&format!(
                "max_ms ({max_ms}) must be at most {MAX_BACKOFF_MS}: the connection is held \
                 while a retry waits"
            )));
        }
        if base_ms > max_ms {
            return Err(refused(&format!("base_ms ({base_ms}) must be at most max_ms ({max_ms})")));
        }
        Ok(Self {
            max_attempts,
            base_ms,
            max_ms,
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

    /// Whether `attempt` (from 1) may be followed by another.
    pub(crate) const fn allows_after(&self, attempt: u32) -> bool {
        attempt < self.max_attempts
    }

    /// The wait before retry number `retry` (from 1), drawn from `entropy`: full jitter,
    /// uniform in `0..=min(max_ms, base_ms · 2^(retry-1))`. The jitter is the whole wait,
    /// never added to it, so no wait exceeds `max_ms`.
    pub(crate) fn backoff(&self, retry: u32, entropy: u64) -> Duration {
        let doublings = retry.saturating_sub(1).min(63);
        let cap = self.base_ms.saturating_mul(1_u64 << doublings).min(self.max_ms);
        Duration::from_millis(entropy % (cap + 1))
    }

    /// [`backoff`](Self::backoff), drawn from a fresh random state.
    pub(crate) fn draw_backoff(&self, retry: u32) -> Duration {
        self.backoff(retry, RandomState::new().hash_one(retry))
    }

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
            .field("base_ms", &self.base_ms)
            .field("max_ms", &self.max_ms)
            .field("observer", &self.observer.is_some())
            .finish()
    }
}

/// How often a mutation's transaction runs when PostgreSQL aborts it.
///
/// Absent, a mutation runs once, as it always has. A retried function runs its body again,
/// so an effect outside the transaction (`dblink`, `pg_background`, a sequence) can repeat:
/// that is why retry is opt-in. An empty section opts in, at the defaults below.
///
/// The server reads it as `[transaction_retry]`, `fraiseql run` as
/// `[database.transaction_retry]` in `fraiseql.toml`:
///
/// ```toml
/// [transaction_retry]
/// max_attempts = 3   # the first attempt included
/// base_ms = 10       # the first retry waits up to this long
/// max_ms = 200       # no wait exceeds this
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct TransactionRetryConfig {
    /// Attempts, the first included. Default: 3.
    pub max_attempts: u32,
    /// The first retry waits a random duration up to this many milliseconds; each later
    /// retry up to twice the one before. Default: 10.
    pub base_ms:      u64,
    /// No wait exceeds this many milliseconds. Default: 200.
    pub max_ms:       u64,
}

impl Default for TransactionRetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_ms:      10,
            max_ms:       200,
        }
    }
}

impl TransactionRetryConfig {
    /// The policy this section describes.
    ///
    /// # Errors
    ///
    /// The bound the section breaks, by name, without the section's own name: each
    /// configuration surface names the section as its operator wrote it.
    pub fn policy(&self) -> std::result::Result<TransactionRetry, String> {
        TransactionRetry::new(self.max_attempts, self.base_ms, self.max_ms).map_err(|e| match e {
            FraiseQLError::Configuration { message } => message,
            other => other.to_string(),
        })
    }
}

fn refused(message: &str) -> FraiseQLError {
    FraiseQLError::Configuration {
        message: message.to_string(),
    }
}

#[cfg(test)]
#[path = "transaction_retry_tests.rs"]
mod tests;
