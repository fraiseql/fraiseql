//! PostgreSQL implementation of checkpoint storage.
//!
//! Provides durable, transactional checkpoint persistence using PostgreSQL.
//! Supports atomic compare-and-swap for multi-listener coordination.

use chrono::Utc;
use sqlx::PgPool;

use super::{CheckpointState, CheckpointStore};
use crate::error::Result;

/// PostgreSQL-backed checkpoint store.
///
/// Provides reliable, durable checkpoint storage using PostgreSQL's
/// transactional guarantees and UPSERT operations.
#[derive(Clone)]
pub struct PostgresCheckpointStore {
    pool: PgPool,
}

impl PostgresCheckpointStore {
    /// Create a new PostgreSQL checkpoint store.
    ///
    /// # Arguments
    ///
    /// * `pool` - PostgreSQL connection pool
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl CheckpointStore for PostgresCheckpointStore {
    async fn load(&self, listener_id: &str) -> Result<Option<CheckpointState>> {
        #[allow(clippy::cast_possible_wrap)] // Reason: checkpoint sequence numbers are positive and won't exceed i64::MAX
        let record = sqlx::query_as::<_, (String, i64, chrono::DateTime<Utc>, i32, i32)>(
            r"
            SELECT listener_id, last_processed_id, last_processed_at, batch_size, event_count
            FROM observer_checkpoints
            WHERE listener_id = $1
            ",
        )
        .bind(listener_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(record.map(
            |(listener_id, last_processed_id, last_processed_at, batch_size, event_count)| {
                CheckpointState {
                    listener_id,
                    last_processed_id,
                    last_processed_at,
                    batch_size: batch_size as usize,
                    event_count: event_count as usize,
                }
            },
        ))
    }

    async fn save(&self, listener_id: &str, state: &CheckpointState) -> Result<()> {
        #[allow(clippy::cast_possible_wrap)] // Reason: checkpoint sequence numbers are positive and won't exceed i64::MAX
        sqlx::query(
            r"
            INSERT INTO observer_checkpoints
                (listener_id, last_processed_id, last_processed_at, batch_size, event_count, updated_at)
            VALUES ($1, $2, $3, $4, $5, NOW())
            ON CONFLICT (listener_id)
            DO UPDATE SET
                last_processed_id = EXCLUDED.last_processed_id,
                last_processed_at = EXCLUDED.last_processed_at,
                batch_size = EXCLUDED.batch_size,
                event_count = EXCLUDED.event_count,
                updated_at = NOW()
            ",
        )
        .bind(listener_id)
        .bind(state.last_processed_id)
        .bind(state.last_processed_at)
        .bind(state.batch_size as i32)
        .bind(state.event_count as i32)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn compare_and_swap(
        &self,
        listener_id: &str,
        expected_id: i64,
        new_id: i64,
    ) -> Result<bool> {
        // Fast path: update an existing row atomically.
        let result = sqlx::query(
            r"
            UPDATE observer_checkpoints
            SET last_processed_id = $3, updated_at = NOW()
            WHERE listener_id = $1 AND last_processed_id = $2
            ",
        )
        .bind(listener_id)
        .bind(expected_id)
        .bind(new_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() > 0 {
            return Ok(true);
        }

        // Edge-case: first-ever checkpoint (expected_id == 0, no row exists yet).
        // INSERT ... ON CONFLICT DO NOTHING ensures concurrent callers are safe:
        // exactly one INSERT wins; all others return rows_affected = 0.
        if expected_id == 0 {
            let inserted = sqlx::query(
                r"
                INSERT INTO observer_checkpoints
                    (listener_id, last_processed_id, last_processed_at, batch_size, event_count, updated_at)
                VALUES ($1, $2, NOW(), 0, 0, NOW())
                ON CONFLICT (listener_id) DO NOTHING
                ",
            )
            .bind(listener_id)
            .bind(new_id)
            .execute(&self.pool)
            .await?;

            return Ok(inserted.rows_affected() > 0);
        }

        Ok(false)
    }

    async fn delete(&self, listener_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM observer_checkpoints WHERE listener_id = $1")
            .bind(listener_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require a test database setup.
    // See the main testing module for integration test infrastructure.

    #[test]
    fn test_checkpoint_store_clone() {
        // Ensure CheckpointStore trait is Clone
        fn assert_clone<T: Clone>() {}
        assert_clone::<PostgresCheckpointStore>();
    }
}
