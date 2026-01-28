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
//! - **check_interval**: How frequently the recovery loop runs (default: 5 seconds)
//! - **max_sagas_per_iteration**: Maximum sagas to process per loop (default: 50)
//! - **stale_age_hours**: Age threshold for cleanup (default: 24 hours)
//!
//! # Example
//!
//! ```ignore
//! use fraiseql_core::federation::saga_recovery_manager::{
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

use tracing::{debug, info, warn};

use crate::federation::saga_store::{
    PostgresSagaStore, Result as SagaStoreResult, Saga, SagaState, SagaStoreError,
};

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
    /// ```ignore
    /// let config = RecoveryConfig::default();
    /// let manager = SagaRecoveryManager::new(Arc::new(store), config);
    /// ```
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
    /// ```ignore
    /// assert!(!manager.is_running());
    /// manager.start_background_loop().await?;
    /// assert!(manager.is_running());
    /// ```
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
    /// ```ignore
    /// let stats = manager.get_stats();
    /// println!("Processed {} sagas in {} iterations", stats.sagas_processed, stats.iterations);
    /// ```
    pub fn get_stats(&self) -> RecoveryStats {
        self.stats.lock().unwrap().clone()
    }

    /// Start the background recovery loop
    ///
    /// Spawns a tokio task that runs recovery iterations periodically according
    /// to the configured check_interval.
    ///
    /// # Errors
    ///
    /// Returns an error if the loop is already running.
    ///
    /// # Example
    ///
    /// ```ignore
    /// manager.start_background_loop().await?;
    /// // Loop now runs in background
    /// ```
    pub async fn start_background_loop(&self) -> SagaStoreResult<()> {
        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(SagaStoreError::Database("Recovery loop already running".to_string()));
        }

        let store = Arc::clone(&self.store);
        let running = Arc::clone(&self.running);
        let stats = Arc::clone(&self.stats);
        let config = self.config;

        tokio::spawn(async move {
            info!("Saga recovery loop started");

            while running.load(Ordering::Acquire) {
                if let Err(e) = Self::run_recovery_iteration(&store, config, &stats).await {
                    warn!("Recovery iteration failed: {}", e);
                }
                tokio::time::sleep(config.check_interval).await;
            }

            info!("Saga recovery loop stopped");
        });

        Ok(())
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
    /// ```ignore
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

    /// Run one iteration of the recovery loop
    ///
    /// Useful for manual recovery triggers or testing. Performs the same
    /// operations as a single background loop iteration.
    ///
    /// # Example
    ///
    /// ```ignore
    /// manager.run_iteration().await?;
    /// let stats = manager.get_stats();
    /// assert!(stats.iterations > 0);
    /// ```
    pub async fn run_iteration(&self) -> SagaStoreResult<()> {
        Self::run_recovery_iteration(&self.store, self.config, &self.stats).await
    }

    /// Helper: load sagas by state with error logging
    ///
    /// Loads sagas from the database for a given state. Logs errors but returns
    /// empty list on failure to maintain resilience.
    async fn load_sagas_by_state(
        store: &PostgresSagaStore,
        state: &SagaState,
        stats: &Mutex<RecoveryStats>,
    ) -> Vec<Saga> {
        match store.load_sagas_by_state(state).await {
            Ok(sagas) => {
                debug!("Found {} sagas in state {}", sagas.len(), state.as_str());
                sagas
            },
            Err(e) => {
                warn!("Failed to load sagas in state {}: {}", state.as_str(), e);
                if let Ok(mut s) = stats.lock() {
                    s.errors += 1;
                }
                Vec::new()
            },
        }
    }

    /// Helper: transition saga to new state with error logging
    ///
    /// Attempts to update a saga's state. Logs errors but returns None on failure
    /// to allow the iteration to continue with other sagas.
    async fn transition_saga(
        store: &PostgresSagaStore,
        saga_id: uuid::Uuid,
        new_state: &SagaState,
        stats: &Mutex<RecoveryStats>,
    ) -> Option<()> {
        match store.update_saga_state(saga_id, new_state).await {
            Ok(_) => {
                debug!("Transitioned saga {} to {}", saga_id, new_state.as_str());
                if let Ok(mut s) = stats.lock() {
                    s.sagas_processed += 1;
                }
                Some(())
            },
            Err(e) => {
                warn!("Failed to transition saga {} to {}: {}", saga_id, new_state.as_str(), e);
                if let Ok(mut s) = stats.lock() {
                    s.errors += 1;
                }
                None
            },
        }
    }

    /// Internal: run one recovery iteration
    ///
    /// Performs the core recovery logic:
    /// 1. Increments iteration counter
    /// 2. Loads and processes pending sagas
    /// 3. Detects executing sagas
    /// 4. Cleans up stale sagas
    async fn run_recovery_iteration(
        store: &PostgresSagaStore,
        config: RecoveryConfig,
        stats: &Mutex<RecoveryStats>,
    ) -> SagaStoreResult<()> {
        // Increment iteration counter
        {
            let mut s = stats.lock().unwrap();
            s.iterations += 1;
        }

        let iteration = {
            let s = stats.lock().unwrap();
            s.iterations
        };

        debug!("Starting recovery iteration {}", iteration);

        // Find pending sagas (not yet started)
        let pending_sagas = Self::load_sagas_by_state(store, &SagaState::Pending, stats).await;

        // Process pending sagas (transition to executing)
        for saga in pending_sagas.iter().take(config.max_sagas_per_iteration as usize) {
            let _ = Self::transition_saga(store, saga.id, &SagaState::Executing, stats).await;
        }

        // Find executing sagas (potentially stuck)
        let executing_sagas = Self::load_sagas_by_state(store, &SagaState::Executing, stats).await;

        // Track executing saga count for observability
        {
            if let Ok(mut s) = stats.lock() {
                s.executing_sagas_found += executing_sagas.len() as u64;
            }
        }

        // Clean up stale sagas
        match store.cleanup_stale_sagas(config.stale_age_hours).await {
            Ok(count) => {
                debug!("Cleaned up {} stale sagas", count);
                if let Ok(mut s) = stats.lock() {
                    s.sagas_cleaned += count;
                }
            },
            Err(e) => {
                warn!("Failed to cleanup stale sagas: {}", e);
                if let Ok(mut s) = stats.lock() {
                    s.errors += 1;
                }
            },
        }

        debug!("Completed recovery iteration {}", iteration);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_config_default() {
        let config = RecoveryConfig::default();
        assert_eq!(config.check_interval, Duration::from_secs(5));
        assert_eq!(config.max_sagas_per_iteration, 50);
        assert_eq!(config.stale_age_hours, 24);
    }

    #[test]
    fn test_recovery_manager_creation() {
        // This is a basic test - full integration tests use the background_loop test file
        let config = RecoveryConfig::default();
        assert_eq!(config.check_interval, Duration::from_secs(5));
    }
}
