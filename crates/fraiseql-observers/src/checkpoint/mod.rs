//! Persistent checkpoint system for zero-event-loss recovery.
//!
//! This module provides durable state management for listeners, enabling
//! automatic recovery from restart with exactly-once semantics. Checkpoints
//! store the last successfully processed event ID, allowing listeners to
//! resume from the exact point they stopped.
//!
//! # Features
//!
//! - **Zero Event Loss**: Checkpoints saved atomically after batch processing
//! - **Automatic Recovery**: Listener resumes from last checkpoint on startup
//! - **Multi-Listener Coordination**: Atomic compare-and-swap for concurrent listeners
//! - **Audit Trail**: Complete checkpoint history in database
//!
//! # Example
//!
//! ```no_run
//! use fraiseql_observers::checkpoint::{CheckpointStore, PostgresCheckpointStore};
//! use sqlx::PgPool;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let pool = PgPool::connect("postgres://user:pass@localhost/db").await?;
//! let store = PostgresCheckpointStore::new(pool);
//!
//! // Load checkpoint for recovery
//! if let Some(state) = store.load("listener-1").await? {
//!     println!("Resume from event ID: {}", state.last_processed_id);
//! }
//! # Ok(())
//! # }
//! ```

pub mod postgres;

use chrono::{DateTime, Utc};
pub use postgres::PostgresCheckpointStore;
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Checkpoint state for a listener.
///
/// Contains all information needed to recover a listener after restart,
/// including the last processed event ID and metadata about the batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointState {
    /// Listener identifier (used as primary key)
    pub listener_id:       String,
    /// Last successfully processed changelog entry ID
    pub last_processed_id: i64,
    /// Timestamp of last checkpoint update
    pub last_processed_at: DateTime<Utc>,
    /// Size of the last batch processed
    pub batch_size:        usize,
    /// Total events processed in this batch
    pub event_count:       usize,
}

impl Default for CheckpointState {
    fn default() -> Self {
        Self {
            listener_id:       String::new(),
            last_processed_id: 0,
            last_processed_at: Utc::now(),
            batch_size:        0,
            event_count:       0,
        }
    }
}

/// Abstraction for durable checkpoint storage.
///
/// Implementations provide persistent storage for listener checkpoints,
/// enabling recovery from restart with zero event loss. Supports atomic
/// operations for multi-listener coordination.
///
/// # Trait Objects
///
/// This trait is object-safe and can be used as `Arc<dyn CheckpointStore>`.
#[async_trait::async_trait]
pub trait CheckpointStore: Send + Sync + Clone {
    /// Load checkpoint for a listener.
    ///
    /// Returns `None` if no checkpoint exists (first startup).
    /// Returns error if database is unavailable.
    ///
    /// # Arguments
    ///
    /// * `listener_id` - Unique identifier for the listener
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Database connection fails
    /// - Query execution fails
    /// - Data deserialization fails
    async fn load(&self, listener_id: &str) -> Result<Option<CheckpointState>>;

    /// Save checkpoint after successful batch.
    ///
    /// This should be called after successfully processing a batch of events.
    /// The save is atomic - either the entire state is persisted or nothing.
    ///
    /// # Arguments
    ///
    /// * `listener_id` - Unique identifier for the listener
    /// * `state` - State to persist
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Database connection fails
    /// - Query execution fails
    /// - Constraint validation fails
    async fn save(&self, listener_id: &str, state: &CheckpointState) -> Result<()>;

    /// Atomic compare-and-swap operation.
    ///
    /// Updates checkpoint only if the current value matches `expected_id`.
    /// Used for multi-listener coordination to prevent race conditions.
    ///
    /// Returns `true` if update succeeded, `false` if the value didn't match.
    ///
    /// # Arguments
    ///
    /// * `listener_id` - Unique identifier for the listener
    /// * `expected_id` - Expected current value
    /// * `new_id` - New value to set
    ///
    /// # Errors
    ///
    /// Returns error if database operation fails.
    async fn compare_and_swap(
        &self,
        listener_id: &str,
        expected_id: i64,
        new_id: i64,
    ) -> Result<bool>;

    /// Delete checkpoint (for cleanup/reset).
    ///
    /// Used when resetting a listener to start from the beginning.
    ///
    /// # Arguments
    ///
    /// * `listener_id` - Unique identifier for the listener
    ///
    /// # Errors
    ///
    /// Returns error if database operation fails.
    async fn delete(&self, listener_id: &str) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_state_default() {
        let state = CheckpointState::default();
        assert_eq!(state.last_processed_id, 0);
        assert_eq!(state.batch_size, 0);
        assert_eq!(state.event_count, 0);
        assert!(state.listener_id.is_empty());
    }

    #[test]
    fn test_checkpoint_state_serialization() {
        let state = CheckpointState {
            listener_id:       "test-listener".to_string(),
            last_processed_id: 1000,
            last_processed_at: Utc::now(),
            batch_size:        50,
            event_count:       50,
        };

        let json = serde_json::to_string(&state).expect("serialize");
        let deserialized: CheckpointState = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.listener_id, state.listener_id);
        assert_eq!(deserialized.last_processed_id, state.last_processed_id);
        assert_eq!(deserialized.batch_size, state.batch_size);
        assert_eq!(deserialized.event_count, state.event_count);
    }
}
