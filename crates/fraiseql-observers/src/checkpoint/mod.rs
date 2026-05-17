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

use std::sync::Arc;

use chrono::{DateTime, Utc};
pub use postgres::PostgresCheckpointStore;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::error::{ObserverError, Result};

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

// ── CheckpointStrategy ────────────────────────────────────────────────────────

/// Delivery-guarantee strategy for observer event processing.
///
/// # Semantics
///
/// | Strategy | Guarantee | Crash behaviour |
/// |----------|-----------|-----------------|
/// | `AtLeastOnce` | At-least-once | On crash between processing and checkpoint, the event is redelivered and processed again |
/// | `EffectivelyOnce` | Effectively-once (idempotent) | Duplicate delivery is detected by the idempotency key and the processing side-effect is suppressed |
///
/// # Why "Effectively-Once" and Not "Exactly-Once"
///
/// True exactly-once delivery requires a distributed transaction that atomically
/// commits **both** the side-effect and the checkpoint in a single operation.
/// This is only achievable when the side-effect itself writes to the same
/// transactional database that stores the checkpoint — e.g., the PostgreSQL
/// `pg_notify` path where both can share one `BEGIN`/`COMMIT`.
///
/// For NATS `JetStream` and other external transports there is no distributed
/// transaction available. `EffectivelyOnce` instead uses an idempotency key
/// (the NATS message ID or a caller-supplied key) stored in a PostgreSQL table
/// before acknowledging the message. If a duplicate arrives, the key lookup
/// returns a hit and the side-effect is skipped, achieving the practical
/// equivalent of exactly-once.
///
/// # Choosing a Strategy
///
/// - **`AtLeastOnce`** (default): suitable for idempotent side-effects such as cache invalidation,
///   search index updates, and best-effort webhook fanout.
/// - **`EffectivelyOnce`**: required for non-idempotent operations such as billing events, audit
///   log writes, and email sends where duplicate execution would be observable by end users.
///
/// # Example
///
/// ```rust
/// use fraiseql_observers::checkpoint::CheckpointStrategy;
///
/// // Default — suitable for cache invalidation, search updates
/// let default_strategy = CheckpointStrategy::default();
/// assert!(matches!(default_strategy, CheckpointStrategy::AtLeastOnce));
///
/// // Effectively-once — for billing events
/// let billing_strategy = CheckpointStrategy::EffectivelyOnce {
///     idempotency_table: "observer_idempotency_keys".to_string(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum CheckpointStrategy {
    /// At-least-once delivery (default).
    ///
    /// The checkpoint is written **after** the processing side-effect completes.
    /// A crash between the side-effect and the checkpoint write causes the event
    /// to be redelivered and processed a second time.
    ///
    /// Use this when side-effects are idempotent (e.g., cache invalidation,
    /// search re-indexing) or when the performance cost of idempotency tracking
    /// is not acceptable.
    #[default]
    AtLeastOnce,

    /// Effectively-once delivery via idempotency key tracking.
    ///
    /// Before processing an event, an idempotency key (derived from the event
    /// message ID or a caller-supplied unique ID) is written to
    /// `idempotency_table`. If the key already exists, the event is a duplicate
    /// and the side-effect is skipped.
    ///
    /// This prevents double-processing at the cost of one extra database
    /// round-trip per event. The idempotency key is **not** removed after
    /// processing — it persists as a deduplication record.
    ///
    /// # Table Schema
    ///
    /// Create the idempotency table before enabling this strategy:
    ///
    /// ```sql
    /// CREATE TABLE observer_idempotency_keys (
    ///     idempotency_key  TEXT        NOT NULL,
    ///     listener_id      TEXT        NOT NULL,
    ///     processed_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ///     PRIMARY KEY (idempotency_key, listener_id)
    /// );
    ///
    /// -- Optional: auto-expire old keys after 7 days to bound table growth
    /// -- (requires pg_cron or a background job)
    /// -- DELETE FROM observer_idempotency_keys WHERE processed_at < NOW() - INTERVAL '7 days';
    /// ```
    EffectivelyOnce {
        /// PostgreSQL table name used to store idempotency keys.
        ///
        /// Defaults to `"observer_idempotency_keys"`. The table must exist
        /// before the listener starts.
        idempotency_table: String,
    },
}

impl CheckpointStrategy {
    /// Returns `true` if this strategy requires an idempotency table.
    #[must_use]
    pub const fn is_effectively_once(&self) -> bool {
        matches!(self, Self::EffectivelyOnce { .. })
    }

    /// Returns the idempotency table name if this is `EffectivelyOnce`.
    #[must_use]
    pub const fn idempotency_table(&self) -> Option<&str> {
        match self {
            Self::AtLeastOnce => None,
            Self::EffectivelyOnce { idempotency_table } => Some(idempotency_table.as_str()),
        }
    }

    /// Creates the idempotency table if it does not already exist.
    ///
    /// Safe to call on every observer startup — uses `CREATE TABLE IF NOT EXISTS`.
    /// When the strategy is `AtLeastOnce`, this is a no-op.
    ///
    /// # Errors
    ///
    /// Returns `ObserverError::DatabaseError` if the database connection fails or the
    /// caller lacks `CREATE TABLE` permissions. The observer should refuse to start
    /// when this returns an error.
    pub async fn create_table_if_not_exists(&self, pool: &sqlx::PgPool) -> Result<()> {
        let Some(table) = self.idempotency_table() else {
            return Ok(()); // AtLeastOnce: no-op
        };

        let sql = format!(
            "CREATE TABLE IF NOT EXISTS {table} (\
               idempotency_key  TEXT        NOT NULL, \
               listener_id      TEXT        NOT NULL, \
               processed_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(), \
               PRIMARY KEY (idempotency_key, listener_id)\
             ); \
             CREATE INDEX IF NOT EXISTS idx_{table}_processed_at \
               ON {table} (processed_at)"
        );

        sqlx::raw_sql(&sql).execute(pool).await.map_err(|e| {
            crate::error::ObserverError::DatabaseError {
                reason: format!("Failed to create idempotency table '{table}': {e}"),
            }
        })?;

        Ok(())
    }

    /// Check if an idempotency key has already been processed.
    ///
    /// Returns `Ok(true)` when the key exists (event is a duplicate → skip).
    /// Returns `Ok(false)` when the key is new (event should be processed).
    /// Returns an error if the database query fails.
    ///
    /// When the strategy is `AtLeastOnce`, always returns `Ok(false)` without
    /// querying the database.
    ///
    /// # Errors
    ///
    /// Returns `ObserverError::Checkpoint` on database failure.
    pub async fn is_duplicate(
        &self,
        pool: &sqlx::PgPool,
        listener_id: &str,
        idempotency_key: &str,
    ) -> Result<bool> {
        let Some(table) = self.idempotency_table() else {
            return Ok(false); // AtLeastOnce: never a duplicate
        };

        // Parameterized query; table name must be a validated identifier.
        // We use a literal interpolation here — the table name comes from
        // the developer's configuration (not user input) so this is safe.
        let sql = format!(
            "SELECT EXISTS(\
               SELECT 1 FROM {table} \
               WHERE idempotency_key = $1 AND listener_id = $2\
             )"
        );

        let exists: bool = sqlx::query_scalar(&sql)
            .bind(idempotency_key)
            .bind(listener_id)
            .fetch_one(pool)
            .await
            .map_err(|e| crate::error::ObserverError::DatabaseError {
                reason: format!("Failed to check idempotency key: {e}"),
            })?;

        Ok(exists)
    }

    /// Record an idempotency key to prevent duplicate processing.
    ///
    /// Uses `INSERT … ON CONFLICT DO NOTHING` so concurrent writers are safe.
    /// Must be called **before** committing the processing side-effect.
    ///
    /// When the strategy is `AtLeastOnce`, this is a no-op.
    ///
    /// # Errors
    ///
    /// Returns `ObserverError::Checkpoint` on database failure.
    pub async fn record_idempotency_key(
        &self,
        pool: &sqlx::PgPool,
        listener_id: &str,
        idempotency_key: &str,
    ) -> Result<()> {
        let Some(table) = self.idempotency_table() else {
            return Ok(()); // AtLeastOnce: no-op
        };

        let sql = format!(
            "INSERT INTO {table} (idempotency_key, listener_id) \
             VALUES ($1, $2) \
             ON CONFLICT (idempotency_key, listener_id) DO NOTHING"
        );

        sqlx::query(&sql)
            .bind(idempotency_key)
            .bind(listener_id)
            .execute(pool)
            .await
            .map_err(|e| crate::error::ObserverError::DatabaseError {
                reason: format!("Failed to record idempotency key: {e}"),
            })?;

        Ok(())
    }
}

// ── CheckpointMode ────────────────────────────────────────────────────────────

/// Whether the checkpoint backend is persistent or ephemeral.
///
/// Pass `CheckpointMode::DevOnly` when constructing [`InMemoryCheckpointStore`]
/// to acknowledge that state will be lost on restart. Call
/// [`check_checkpoint_requirement`] at startup to enforce that production
/// environments never accidentally use the in-memory store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CheckpointMode {
    /// Explicitly acknowledged development/test mode.
    /// Checkpoint state is **not** durable — it is lost on every restart.
    DevOnly,
    /// Durable storage (e.g. [`PostgresCheckpointStore`]).
    Persistent,
}

// ── InMemoryCheckpointStore ───────────────────────────────────────────────────

/// In-memory checkpoint store for **development and testing only**.
///
/// # Warning
///
/// **Do not use in production.** All checkpoint state is discarded on restart,
/// causing every listener to reprocess events from the beginning.
/// Use [`PostgresCheckpointStore`] for production deployments.
///
/// Construction via [`InMemoryCheckpointStore::new`] emits a `warn!` log so
/// the choice is always visible in log output. Use
/// [`InMemoryCheckpointStore::new_silent`] in unit tests where the warning
/// adds noise.
///
/// Call [`check_checkpoint_requirement`] at application startup to fail-hard
/// when `FRAISEQL_CHECKPOINT_REQUIRE_PERSISTENT=true` is set.
#[derive(Clone)]
pub struct InMemoryCheckpointStore {
    store: Arc<dashmap::DashMap<String, CheckpointState>>,
}

impl InMemoryCheckpointStore {
    /// Create a new in-memory store and emit a `warn!` log.
    #[must_use]
    pub fn new() -> Self {
        warn!(
            "InMemoryCheckpointStore is active — checkpoint state will NOT survive restart. \
             Use PostgresCheckpointStore in production."
        );
        Self::new_silent()
    }

    /// Create a new in-memory store **without** emitting a warning log.
    ///
    /// Only use this in unit tests where the warning would add noise.
    #[must_use]
    pub fn new_silent() -> Self {
        Self {
            store: Arc::new(dashmap::DashMap::new()),
        }
    }
}

impl Default for InMemoryCheckpointStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl CheckpointStore for InMemoryCheckpointStore {
    async fn load(&self, listener_id: &str) -> Result<Option<CheckpointState>> {
        Ok(self.store.get(listener_id).map(|v| v.clone()))
    }

    async fn save(&self, listener_id: &str, state: &CheckpointState) -> Result<()> {
        self.store.insert(listener_id.to_string(), state.clone());
        Ok(())
    }

    /// Atomic compare-and-swap using `DashMap`'s entry API.
    ///
    /// **Edge-case**: when `expected_id == 0` and no entry exists, the call
    /// succeeds and inserts the first checkpoint. This matches the behaviour of
    /// [`PostgresCheckpointStore::compare_and_swap`] for first-ever saves.
    async fn compare_and_swap(
        &self,
        listener_id: &str,
        expected_id: i64,
        new_id: i64,
    ) -> Result<bool> {
        use dashmap::Entry;
        match self.store.entry(listener_id.to_string()) {
            Entry::Vacant(e) => {
                // No checkpoint exists yet. Succeed only when starting from zero.
                if expected_id == 0 {
                    e.insert(CheckpointState {
                        listener_id:       listener_id.to_string(),
                        last_processed_id: new_id,
                        last_processed_at: Utc::now(),
                        batch_size:        0,
                        event_count:       0,
                    });
                    Ok(true)
                } else {
                    Ok(false)
                }
            },
            Entry::Occupied(mut e) => {
                if e.get().last_processed_id == expected_id {
                    e.get_mut().last_processed_id = new_id;
                    e.get_mut().last_processed_at = Utc::now();
                    Ok(true)
                } else {
                    Ok(false)
                }
            },
        }
    }

    async fn delete(&self, listener_id: &str) -> Result<()> {
        self.store.remove(listener_id);
        Ok(())
    }
}

// ── Startup guard ─────────────────────────────────────────────────────────────

/// Check whether the current environment requires a persistent checkpoint store.
///
/// If the environment variable `FRAISEQL_CHECKPOINT_REQUIRE_PERSISTENT` is set
/// to `true`, `1`, or `yes` **and** `mode` is [`CheckpointMode::DevOnly`], this
/// function returns an error with an operator-actionable message.
///
/// Call this once at application startup, after choosing the checkpoint backend:
///
/// ```rust
/// use fraiseql_observers::checkpoint::{CheckpointMode, check_checkpoint_requirement};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// check_checkpoint_requirement(CheckpointMode::DevOnly)?; // fails in prod
/// check_checkpoint_requirement(CheckpointMode::Persistent)?; // always ok
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Returns [`ObserverError::InvalidConfig`] when `FRAISEQL_CHECKPOINT_REQUIRE_PERSISTENT`
/// is truthy and `mode` is `DevOnly`.
pub fn check_checkpoint_requirement(mode: CheckpointMode) -> Result<()> {
    if mode == CheckpointMode::DevOnly {
        let required = std::env::var("FRAISEQL_CHECKPOINT_REQUIRE_PERSISTENT")
            .is_ok_and(|v| matches!(v.to_lowercase().as_str(), "true" | "1" | "yes"));

        if required {
            return Err(ObserverError::InvalidConfig {
                message:
                    "FRAISEQL_CHECKPOINT_REQUIRE_PERSISTENT is set but InMemoryCheckpointStore \
                     is in use. Configure PostgresCheckpointStore for production deployments."
                        .to_string(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
