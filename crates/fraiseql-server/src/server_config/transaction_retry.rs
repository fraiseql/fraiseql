//! `[transaction_retry]`: re-running a mutation's transaction that PostgreSQL aborts as a
//! deadlock victim (`40P01`) or a serialization failure (`40001`) (#1879).

use std::sync::Arc;

use fraiseql_core::db::postgres::{RetryEvent, RetryObserver, TransactionRetry};

use super::ServerConfig;

impl ServerConfig {
    /// The policy `[transaction_retry]` describes: one attempt when it is absent.
    ///
    /// # Errors
    ///
    /// A message naming the bound `[transaction_retry]` breaks.
    pub fn transaction_retry(&self) -> Result<TransactionRetry, String> {
        self.transaction_retry.as_ref().map_or_else(
            || Ok(TransactionRetry::off()),
            |section| section.policy().map_err(|e| format!("[transaction_retry] {e}")),
        )
    }
}

/// The counter a retry decision increments, and its labels.
#[must_use]
pub fn retry_metric(event: RetryEvent<'_>) -> (&'static str, [(&'static str, String); 2]) {
    let (name, function, sql_state) = match event {
        RetryEvent::Retrying {
            function,
            sql_state,
            ..
        } => ("fraiseql_mutation_transaction_retries_total", function, sql_state),
        RetryEvent::Exhausted {
            function,
            sql_state,
            ..
        } => ("fraiseql_mutation_transaction_retries_exhausted_total", function, sql_state),
    };
    (
        name,
        [
            ("function", function.to_string()),
            ("sqlstate", sql_state.to_string()),
        ],
    )
}

/// Count each retry decision on its [`retry_metric`] counter.
#[must_use]
pub fn retry_metrics_observer() -> RetryObserver {
    Arc::new(|event| {
        let (name, [(function_key, function), (sql_state_key, sql_state)]) = retry_metric(event);
        metrics::counter!(name, function_key => function, sql_state_key => sql_state).increment(1);
    })
}
