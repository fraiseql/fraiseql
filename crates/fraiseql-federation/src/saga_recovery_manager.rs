//! Saga Recovery Manager for distributed transaction crash recovery.
//!
//! Manages background recovery of in-flight sagas with periodic detection,
//! state transitions, and cleanup of stale sagas. Provides resilient recovery
//! that continues gracefully through individual saga failures.
//!
//! # Overview
//!
//! The recovery manager runs a background loop that:
//! 1. **Detects pending sagas** - Finds sagas that haven't started yet
//! 2. **Processes pending sagas** - Transitions them to executing state (up to batch size)
//! 3. **Detects executing sagas** - Identifies potentially stuck/in-flight sagas
//! 4. **Cleans up stale sagas** - Removes sagas older than configured grace period
//!
//! The recovery process is **resilient**: if a single saga fails to process, the
//! loop continues with the next saga rather than aborting the iteration.
//!
//! # State Machine
//!
//! ```text
//! Pending ──Recovery──> Executing ──[Completion]──> Completed
//! Executing (stuck)              ──[Failure]──────> Failed
//! Completed/Failed ──[Age Threshold]──> Cleaned
//! ```
//!
//! # Configuration
//!
//! - **`check_interval`**: How frequently the recovery loop runs (default: 5 seconds)
//! - **`max_sagas_per_iteration`**: Maximum sagas to process per loop (default: 50)
//! - **`stale_age_hours`**: Age threshold for cleanup (default: 24 hours)
//!
//! # Example
//!
//! ```text
//! // Requires: distributed saga infrastructure (PostgreSQL + message broker).
//! // See: tests/integration/ for runnable examples.
//! use fraiseql_federation::saga_recovery_manager::{
//!     SagaRecoveryManager, RecoveryConfig,
//! };
//! use std::sync::Arc;
//!
//! let config = RecoveryConfig {
//!     check_interval: Duration::from_secs(10),
//!     max_sagas_per_iteration: 100,
//!     stale_age_hours: 48,
//! };
//!
//! let manager = SagaRecoveryManager::new(
//!     Arc::new(saga_store),
//!     config,
//! );
//!
//! // Start background recovery loop
//! manager.start_background_loop().await?;
//!
//! // Run manual iteration (useful for testing)
//! manager.run_iteration().await?;
//!
//! // Check loop status
//! assert!(manager.is_running());
//!
//! // Stop gracefully
//! manager.stop_background_loop().await?;
//! ```

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use ::tracing::{info, warn};
use fraiseql_db::traits::DatabaseAdapter;
use uuid::Uuid;

use crate::{
    mutation_executor::FederationMutationExecutor,
    saga_executor::SagaExecutor,
    saga_store::{PostgresSagaStore, Result as SagaStoreResult, Saga, SagaStoreError},
};

/// Pure recovery-phase decision helpers.
mod recovery;

/// Configuration for saga recovery manager
///
/// Controls the behavior and tuning parameters of the recovery manager.
#[derive(Debug, Clone, Copy)]
pub struct RecoveryConfig {
    /// Interval between recovery loop iterations
    ///
    /// Determines how frequently the background loop checks for pending/executing sagas.
    /// Smaller values detect and recover sagas faster but consume more resources.
    pub check_interval:          Duration,
    /// Maximum sagas to process per iteration
    ///
    /// Limits the number of sagas transitioned per iteration to avoid overwhelming
    /// the database and keeping iteration time bounded.
    pub max_sagas_per_iteration: u32,
    /// Grace period before marking sagas as stale (hours)
    ///
    /// Sagas older than this duration are considered stale and eligible for cleanup.
    pub stale_age_hours:         i64,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            check_interval:          Duration::from_secs(5),
            max_sagas_per_iteration: 50,
            stale_age_hours:         24,
        }
    }
}

/// Metrics tracked by the recovery manager
///
/// Provides observability into recovery operations for monitoring and debugging.
#[derive(Debug, Clone, Default)]
pub struct RecoveryStats {
    /// Total iterations executed
    pub iterations:            u64,
    /// Total pending sagas processed
    pub sagas_processed:       u64,
    /// Total executing sagas detected
    pub executing_sagas_found: u64,
    /// Total stale sagas cleaned up
    pub sagas_cleaned:         u64,
    /// Total errors encountered
    pub errors:                u64,
}

/// Saga Recovery Manager
///
/// Manages background recovery of in-flight sagas, detecting stuck transactions
/// and cleaning up completed ones. Designed for production use with multiple
/// federation instances running concurrently.
///
/// # Thread Safety
///
/// All methods are thread-safe and can be called concurrently. The manager uses:
/// - `Arc<AtomicBool>` for lock-free state checking
/// - `Arc<Mutex<T>>` for protected counter access
/// - `Arc<PostgresSagaStore>` for shared database access
///
/// # Error Handling
///
/// The recovery manager is resilient to errors:
/// - Individual saga failures don't stop the iteration
/// - Database errors are logged but don't prevent cleanup
/// - The background loop continues despite transient failures
pub struct SagaRecoveryManager {
    store:   Arc<PostgresSagaStore>,
    config:  RecoveryConfig,
    running: Arc<AtomicBool>,
    stats:   Arc<Mutex<RecoveryStats>>,
}

impl SagaRecoveryManager {
    /// Create a new saga recovery manager
    ///
    /// # Arguments
    ///
    /// * `store` - PostgreSQL saga store
    /// * `config` - Recovery manager configuration
    ///
    /// # Example
    ///
    /// ```text
    /// // Requires: distributed saga infrastructure (PostgreSQL + message broker).
    /// // See: tests/integration/ for runnable examples.
    /// let config = RecoveryConfig::default();
    /// let manager = SagaRecoveryManager::new(Arc::new(store), config);
    /// ```
    #[must_use]
    pub fn new(store: Arc<PostgresSagaStore>, config: RecoveryConfig) -> Self {
        Self {
            store,
            config,
            running: Arc::new(AtomicBool::new(false)),
            stats: Arc::new(Mutex::new(RecoveryStats::default())),
        }
    }

    /// Check if background loop is running
    ///
    /// Returns true if the background recovery loop is actively running.
    /// Uses lock-free atomic read for high performance.
    ///
    /// # Example
    ///
    /// ```text
    /// // Requires: distributed saga infrastructure (PostgreSQL + message broker).
    /// // See: tests/integration/ for runnable examples.
    /// assert!(!manager.is_running());
    /// manager.start_background_loop().await?;
    /// assert!(manager.is_running());
    /// ```
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Get current recovery statistics
    ///
    /// Returns a snapshot of metrics tracked during recovery operations.
    /// Useful for monitoring and debugging recovery behavior.
    ///
    /// # Example
    ///
    /// ```text
    /// // Requires: distributed saga infrastructure (PostgreSQL + message broker).
    /// // See: tests/integration/ for runnable examples.
    /// let stats = manager.get_stats();
    /// println!("Processed {} sagas in {} iterations", stats.sagas_processed, stats.iterations);
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the internal stats mutex is poisoned (a prior panic occurred
    /// while the lock was held).
    #[must_use]
    pub fn get_stats(&self) -> RecoveryStats {
        self.stats.lock().expect("stats mutex poisoned").clone()
    }

    /// Stop the background recovery loop
    ///
    /// Gracefully stops the background loop. The loop exits after the current
    /// iteration completes.
    ///
    /// # Errors
    ///
    /// Returns an error if the loop is not currently running.
    ///
    /// # Example
    ///
    /// ```text
    /// // Requires: distributed saga infrastructure (PostgreSQL + message broker).
    /// // See: tests/integration/ for runnable examples.
    /// manager.stop_background_loop().await?;
    /// // Loop stops after current iteration
    /// ```
    pub async fn stop_background_loop(&self) -> SagaStoreResult<()> {
        if self
            .running
            .compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(SagaStoreError::Database("Recovery loop not running".to_string()));
        }
        Ok(())
    }
}

/// Crash-recovery driver: claim stuck sagas under a lease via `SELECT … FOR UPDATE
/// SKIP LOCKED` and replay them through [`SagaExecutor::execute_saga`]. Recovery replay
/// is local-only — it passes no subgraph registry / HTTP client — so re-driving a
/// crash-interrupted *remote* step is deferred (documented on `recover_one`).
impl SagaRecoveryManager {
    /// Run one recovery tick: find crash-interrupted sagas and re-drive each to
    /// a terminal state.
    ///
    /// Stuck sagas (left [`SagaState::Executing`](crate::saga_store::SagaState)
    /// by a crash, bounded by `max_sagas_per_iteration`) and pending sagas
    /// (never started) are each recorded for recovery and replayed through
    /// [`SagaExecutor::execute_saga`]. The tick is **resilient**: a single
    /// saga's replay error is logged and counted (`stats.errors`) but never
    /// aborts the iteration — the remaining sagas are still processed. Finally
    /// terminal sagas past the stale threshold are cleaned up.
    ///
    /// # Arguments
    ///
    /// * `executor` - Local mutation transport used to replay each saga's steps
    ///
    /// # Errors
    ///
    /// Returns [`SagaStoreError`] only if the initial store scans
    /// (`claim_stuck_sagas` / `find_pending_sagas`) fail; per-saga replay and
    /// cleanup failures are counted in `stats` rather than propagated.
    ///
    /// # Panics
    ///
    /// Panics if the internal stats mutex is poisoned (a prior panic occurred
    /// while the lock was held).
    pub async fn run_iteration<A: DatabaseAdapter>(
        &self,
        executor: &FederationMutationExecutor<A>,
    ) -> SagaStoreResult<()> {
        let saga_executor = SagaExecutor::with_store(Arc::clone(&self.store));

        // Stuck = sagas a crash left Executing. Claim up to
        // `max_sagas_per_iteration` of them under a fresh per-iteration worker id
        // and a lease, so two recovery workers ticking at once claim disjoint
        // sets and never double-drive a saga (FOR UPDATE SKIP LOCKED). The lease
        // outlives one iteration (10× the poll interval, floored at 60s) so this
        // worker keeps its claims while re-driving; a crashed worker's claims
        // lapse and become reclaimable.
        let worker_id = Uuid::new_v4();
        let lease_secs =
            i64::try_from(self.config.check_interval.as_secs().saturating_mul(10).max(60))
                .unwrap_or(i64::MAX);
        let limit = i64::from(self.config.max_sagas_per_iteration);
        let stuck = self.store.claim_stuck_sagas(worker_id, lease_secs, limit).await?;
        let executing_found = u64::try_from(stuck.len()).unwrap_or(u64::MAX);

        // Pending = sagas that were persisted but never started executing.
        let pending = self.store.find_pending_sagas().await?;

        let mut processed: u64 = 0;
        let mut errors: u64 = 0;

        // Only genuinely in-flight sagas are re-driven; `saga_is_recoverable`
        // guards against a future store scan surfacing a terminal saga.
        for saga in stuck
            .iter()
            .chain(pending.iter())
            .filter(|saga| recovery::saga_is_recoverable(&saga.state))
        {
            processed += 1;
            if let Err(error) = self.recover_one(&saga_executor, executor, saga).await {
                warn!(saga_id = %saga.id, error = ?error, "saga recovery attempt failed; continuing");
                errors += 1;
            }
        }

        // Clean up terminal sagas past the stale threshold, after replay attempts.
        let cleaned = match self.store.cleanup_stale_sagas(self.config.stale_age_hours).await {
            Ok(count) => count,
            Err(error) => {
                warn!(error = ?error, "stale saga cleanup failed");
                errors += 1;
                0
            },
        };

        // Commit this iteration's counters in one locked section.
        {
            let mut stats = self.stats.lock().expect("stats mutex poisoned");
            stats.iterations += 1;
            stats.sagas_processed += processed;
            stats.executing_sagas_found += executing_found;
            stats.sagas_cleaned += cleaned;
            stats.errors += errors;
        }

        Ok(())
    }

    /// Record a recovery attempt for `saga` and replay its forward execution.
    ///
    /// Persists a crash-recovery record (`mark_saga_for_recovery`) for the audit
    /// trail, logs the attempt, then drives the saga through
    /// [`SagaExecutor::execute_saga`], which transitions it to a terminal
    /// `Completed`/`Failed` state.
    async fn recover_one<A: DatabaseAdapter>(
        &self,
        saga_executor: &SagaExecutor,
        executor: &FederationMutationExecutor<A>,
        saga: &Saga,
    ) -> SagaStoreResult<()> {
        self.store.mark_saga_for_recovery(saga.id, "auto-recovery").await?;

        let attempt = u32::try_from(self.store.get_recovery_attempts(saga.id).await?).unwrap_or(0);
        info!("{}", recovery::recovery_log_line(saga.id, attempt));

        // Recovery replays through the local dispatch path only: crash recovery
        // re-drives steps against the local SQL adapter (remote saga recovery is
        // future work), so no subgraph registry / HTTP client / @requires entity
        // resolver is passed.
        saga_executor
            .execute_saga(saga.id, executor, &std::collections::HashMap::new(), None, None)
            .await?;
        Ok(())
    }

    /// Start the background recovery loop as a spawned Tokio task.
    ///
    /// Compare-and-swaps the `running` flag `false → true` (a second call while
    /// running fails loud rather than spawning a duplicate loop), then spawns a
    /// task that runs [`Self::run_iteration`] every `check_interval`. The
    /// loop exits promptly once [`Self::stop_background_loop`] clears the flag.
    /// Per-iteration errors are logged and the loop keeps running.
    ///
    /// # Arguments
    ///
    /// * `executor` - Local mutation transport shared with the spawned loop
    ///
    /// # Errors
    ///
    /// Returns [`SagaStoreError::Database`] if the loop is already running.
    pub async fn start_background_loop<A>(
        self: Arc<Self>,
        executor: Arc<FederationMutationExecutor<A>>,
    ) -> SagaStoreResult<()>
    where
        A: DatabaseAdapter + 'static,
    {
        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(SagaStoreError::Database("Recovery loop already running".to_string()));
        }

        let period = self.config.check_interval;
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(period);
            while self.running.load(Ordering::Acquire) {
                ticker.tick().await;
                // Re-check after the (possibly long) tick so a stop requested
                // mid-wait takes effect before the next iteration runs.
                if !self.running.load(Ordering::Acquire) {
                    break;
                }
                if let Err(error) = self.run_iteration(executor.as_ref()).await {
                    warn!(error = ?error, "saga recovery iteration failed; loop continues");
                }
            }
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests;
