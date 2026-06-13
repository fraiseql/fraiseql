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

use ::tracing::info;

use crate::saga_store::{PostgresSagaStore, Result as SagaStoreResult, SagaStoreError};

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
    // Reason: retained as the real dependencies a wired recovery implementation
    // needs; the recovery loop currently fails loud (M-saga-recovery) and reads
    // neither, but `new` accepts both as the stable public construction contract.
    #[allow(dead_code)]
    store:   Arc<PostgresSagaStore>,
    #[allow(dead_code)]
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

    /// Start the background recovery loop.
    ///
    /// # Status
    ///
    /// **Not implemented.** The background loop previously transitioned every
    /// `Pending` saga to `Executing` while executing nothing — flipping saga
    /// state without performing any recovery work (audit M-saga-recovery). It
    /// now fails loud and does **not** start a loop or flip the running flag, so
    /// it can never silently mutate saga state.
    ///
    /// # Errors
    ///
    /// Always returns [`SagaStoreError::NotImplemented`].
    pub async fn start_background_loop(&self) -> SagaStoreResult<()> {
        info!("Saga recovery loop requested but distributed saga recovery is unwired");
        Err(SagaStoreError::NotImplemented {
            operation: "SagaRecoveryManager::start_background_loop".to_string(),
        })
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

    /// Run one iteration of the recovery loop.
    ///
    /// # Status
    ///
    /// **Not implemented.** A single iteration previously transitioned every
    /// `Pending` saga to `Executing` while executing nothing — flipping saga
    /// state without performing any recovery work (audit M-saga-recovery). It
    /// now fails loud and mutates no saga state.
    ///
    /// # Errors
    ///
    /// Always returns [`SagaStoreError::NotImplemented`].
    pub async fn run_iteration(&self) -> SagaStoreResult<()> {
        info!("Saga recovery iteration requested but distributed saga recovery is unwired");
        Err(SagaStoreError::NotImplemented {
            operation: "SagaRecoveryManager::run_recovery".to_string(),
        })
    }
}

#[cfg(test)]
mod tests;
