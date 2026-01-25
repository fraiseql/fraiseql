//! Transaction boundary management for webhook processing.
//!
//! This module ensures correct transaction isolation for webhook handlers:
//! 1. Signature verification: No transaction (fail fast)
//! 2. Idempotency check: Read-only transaction
//! 3. Event processing: Single transaction with idempotency record + handler

use futures::future::BoxFuture;
use sqlx::{PgPool, Postgres, Transaction};

use super::{Result, WebhookError};

/// Transaction isolation levels for webhook processing
#[derive(Debug, Clone, Copy, Default)]
pub enum WebhookIsolation {
    /// Read Committed - default, good for most cases
    #[default]
    ReadCommitted,
    /// Repeatable Read - for handlers that read-then-write
    RepeatableRead,
    /// Serializable - for handlers with complex consistency requirements
    Serializable,
}

impl WebhookIsolation {
    /// Convert to SQL isolation level string
    #[must_use]
    pub fn as_sql(self) -> &'static str {
        match self {
            Self::ReadCommitted => "READ COMMITTED",
            Self::RepeatableRead => "REPEATABLE READ",
            Self::Serializable => "SERIALIZABLE",
        }
    }
}

/// Execute webhook handler within a transaction with specified isolation level.
///
/// # Critical
///
/// The idempotency record and event handler MUST be in the same transaction:
/// - If handler succeeds but idempotency update fails → duplicate processing on retry
/// - If idempotency records but handler fails → event marked processed but not handled
///
/// # Errors
///
/// Returns `WebhookError::Database` if transaction fails to start, commit, or during handler
/// execution.
pub async fn execute_in_transaction<F, T>(
    pool: &PgPool,
    isolation: WebhookIsolation,
    f: F,
) -> Result<T>
where
    F: for<'c> FnOnce(&'c mut Transaction<'_, Postgres>) -> BoxFuture<'c, Result<T>>,
{
    let mut tx = pool.begin().await.map_err(|e| WebhookError::Database(e.to_string()))?;

    // Set isolation level
    sqlx::query(&format!("SET TRANSACTION ISOLATION LEVEL {}", isolation.as_sql()))
        .execute(&mut *tx)
        .await
        .map_err(|e| WebhookError::Database(e.to_string()))?;

    let result = f(&mut tx).await;

    match result {
        Ok(value) => {
            tx.commit().await.map_err(|e| WebhookError::Database(e.to_string()))?;
            Ok(value)
        },
        Err(e) => {
            // Explicit rollback (also happens on drop, but be explicit)
            let _ = tx.rollback().await;
            Err(e)
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_isolation_level_sql() {
        assert_eq!(WebhookIsolation::ReadCommitted.as_sql(), "READ COMMITTED");
        assert_eq!(WebhookIsolation::RepeatableRead.as_sql(), "REPEATABLE READ");
        assert_eq!(WebhookIsolation::Serializable.as_sql(), "SERIALIZABLE");
    }

    #[test]
    fn test_default_isolation_level() {
        let default = WebhookIsolation::default();
        assert_eq!(default.as_sql(), "READ COMMITTED");
    }
}
