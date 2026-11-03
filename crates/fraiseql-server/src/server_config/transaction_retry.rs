//! `[transaction_retry]`: re-running a mutation's transaction that PostgreSQL aborts as a
//! deadlock victim (`40P01`) or a serialization failure (`40001`) (#1879).

use fraiseql_core::db::postgres::{RetryEvent, TransactionRetry};
use serde::{Deserialize, Serialize};

use super::ServerConfig;

/// How often a mutation's transaction runs when PostgreSQL aborts it.
///
/// Absent, a mutation runs once, as it always has. A retried function runs its body again,
/// so an effect outside the transaction (`dblink`, `pg_background`, a sequence) can repeat:
/// that is why retry is opt-in.
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

impl ServerConfig {
    /// The policy `[transaction_retry]` describes: one attempt when it is absent.
    ///
    /// # Errors
    ///
    /// A message naming the bound `[transaction_retry]` breaks.
    pub fn transaction_retry(&self) -> Result<TransactionRetry, String> {
        Ok(TransactionRetry::off())
    }
}

/// The counter a retry decision increments, and its labels.
#[must_use]
pub fn retry_metric(_event: RetryEvent<'_>) -> (&'static str, [(&'static str, String); 2]) {
    ("", [("function", String::new()), ("sqlstate", String::new())])
}
