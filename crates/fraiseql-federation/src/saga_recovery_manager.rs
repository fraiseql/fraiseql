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
    /// How long a saga's row must sit untouched before it counts as **stuck**
    /// (#745).
    ///
    /// A live forward drive heartbeats the saga row on every step transition, so
    /// only a saga whose `updated_at` is older than this threshold — i.e. one
    /// whose driver stopped moving it — is claimable by the recovery loop. Set it
    /// comfortably above the longest expected single-step duration (a slow step
    /// stalls the heartbeat for its whole dispatch); the default is 5 minutes.
    /// The same threshold gates pending-saga pickup, so a saga in its creator's
    /// `create_saga` → `execute_saga` window is not re-driven concurrently.
    pub stuck_threshold:         Duration,
    /// Maximum automatic recovery attempts per saga before it is **parked** for
    /// manual recovery (#785).
    ///
    /// Each recovery replay records a genuine attempt count; once a saga has
    /// been attempted this many times it is parked (its lease pushed to
    /// infinity) instead of being retried forever, and an operator resolves it.
    pub max_recovery_attempts:   u32,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            check_interval:          Duration::from_secs(5),
            max_sagas_per_iteration: 50,
            stale_age_hours:         24,
            stuck_threshold:         Duration::from_mins(5),
            max_recovery_attempts:   5,
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
    routing: Option<RecoveryRouting>,
}

/// The remote-dispatch transport a recovery worker replays remote steps on:
/// the same registry/client/resolver triple the originating coordinator held.
///
/// Without it, a recovery worker refuses to replay any saga containing a
/// remote step (#766) — it parks the saga for manual recovery rather than
/// silently executing another service's mutation against the local database.
pub struct RecoveryRouting {
    /// Registered remote peers: subgraph name → validated base URL.
    pub subgraph_urls:   std::collections::HashMap<String, reqwest::Url>,
    /// HTTP client for remote step dispatch.
    pub http_client:     crate::mutation_http_client::HttpMutationClient,
    /// Entity resolver for cross-subgraph `@requires` pre-fetch, if configured.
    pub entity_resolver: Option<crate::http_resolver::HttpEntityResolver>,
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
            routing: None,
        }
    }

    /// Give this recovery worker the remote-dispatch transport (builder).
    ///
    /// With routing, a claimed saga's remote steps are re-driven over HTTPS to
    /// their registered peer — the same transport the originating coordinator
    /// used. Without it, any saga containing a remote step is **parked for
    /// manual recovery** instead of replayed: replaying it here would silently
    /// execute the remote mutation against the local database (#766).
    #[must_use]
    pub fn with_routing(mut self, routing: RecoveryRouting) -> Self {
        self.routing = Some(routing);
        self
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
/// SKIP LOCKED` and replay them through [`SagaExecutor::execute_saga`]. Remote steps
/// are replayed on the transport configured via [`SagaRecoveryManager::with_routing`];
/// without routing, a saga containing a remote step is parked for manual recovery
/// rather than replayed against the local adapter (#766).
impl SagaRecoveryManager {
    /// Run one recovery tick: find crash-interrupted sagas and re-drive each to
    /// a terminal state.
    ///
    /// Stuck sagas (left [`SagaState::Executing`](crate::saga_store::SagaState)
    /// by a crash — **stale** past `stuck_threshold`, never merely executing,
    /// #745 — bounded by `max_sagas_per_iteration`) and stale pending sagas
    /// (never started) are each recorded for recovery and replayed through
    /// [`SagaExecutor::execute_saga`]. Replay of a `Completed` step is a
    /// synthesized skip (#744); a saga whose remote steps this worker cannot
    /// reach, or that exhausted `max_recovery_attempts`, is parked for manual
    /// recovery. The tick is **resilient**: a single saga's replay error is
    /// logged and counted (`stats.errors`) but never aborts the iteration — the
    /// remaining sagas are still processed. Finally terminal sagas past the
    /// stale threshold are cleaned up.
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

        // Stuck = sagas a crash left Executing AND whose row has gone stale
        // (their driver stopped heart-beating it, #745). Claim up to
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
        let stuck_secs = i64::try_from(self.config.stuck_threshold.as_secs()).unwrap_or(i64::MAX);
        let stuck = self.store.claim_stuck_sagas(worker_id, lease_secs, limit, stuck_secs).await?;
        let executing_found = u64::try_from(stuck.len()).unwrap_or(u64::MAX);

        // Pending = sagas that were persisted but never started executing, past
        // the same staleness gate (a fresh Pending saga is its creator's to run).
        let pending = self.store.find_pending_sagas(stuck_secs).await?;

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
    /// Guards before any replay:
    /// - a saga past `max_recovery_attempts` is **parked** for manual recovery (its lease pushed to
    ///   infinity) instead of retried forever (#785);
    /// - a saga containing a remote step this worker has no transport for is parked too — replaying
    ///   it would execute the remote mutation against the local database (#766). Configure
    ///   [`Self::with_routing`] to re-drive remote steps on their real transport.
    ///
    /// Then persists a crash-recovery record (`mark_saga_for_recovery`, a real
    /// incrementing attempt count) and drives the saga through
    /// [`SagaExecutor::execute_saga`], which transitions it to a terminal
    /// `Completed`/`Failed` state (skipping already-`Completed` steps, #744).
    async fn recover_one<A: DatabaseAdapter>(
        &self,
        saga_executor: &SagaExecutor,
        executor: &FederationMutationExecutor<A>,
        saga: &Saga,
    ) -> SagaStoreResult<()> {
        // Attempt cap: park rather than retry a poison saga forever (#785).
        let prior_attempts =
            u32::try_from(self.store.get_recovery_attempts(saga.id).await?).unwrap_or(u32::MAX);
        if prior_attempts >= self.config.max_recovery_attempts {
            self.store.park_saga_for_manual_recovery(saga.id).await?;
            return Err(SagaStoreError::Database(format!(
                "saga {} exceeded max_recovery_attempts ({}); parked for manual recovery",
                saga.id, self.config.max_recovery_attempts
            )));
        }

        // Transport pre-flight (#766): every remote step still ahead of us must
        // be reachable on THIS worker's routing, or the saga must not be
        // replayed at all — an unreachable remote step must fail loud here, not
        // fall through to the local SQL adapter mid-replay.
        let steps = self.store.load_saga_steps(saga.id).await?;
        let unreachable: Vec<&str> = steps
            .iter()
            .filter(|step| {
                step.remote
                    && !matches!(
                        step.state,
                        crate::saga_store::StepState::Completed
                            | crate::saga_store::StepState::Compensated
                    )
                    && !self
                        .routing
                        .as_ref()
                        .is_some_and(|r| r.subgraph_urls.contains_key(&step.subgraph))
            })
            .map(|step| step.subgraph.as_str())
            .collect();
        if !unreachable.is_empty() {
            self.store.park_saga_for_manual_recovery(saga.id).await?;
            return Err(SagaStoreError::Database(format!(
                "saga {} has remote steps on subgraph(s) {:?} but this recovery worker has no \
                 transport for them; parked for manual recovery (configure \
                 SagaRecoveryManager::with_routing to re-drive remote steps)",
                saga.id, unreachable
            )));
        }

        self.store.mark_saga_for_recovery(saga.id, "auto-recovery").await?;

        let attempt = u32::try_from(self.store.get_recovery_attempts(saga.id).await?).unwrap_or(0);
        info!("{}", recovery::recovery_log_line(saga.id, attempt));

        // Replay on the same routing the forward drive would use: remote steps
        // go to their registered peer over the configured client; local steps
        // run on the local SQL adapter.
        let empty = std::collections::HashMap::new();
        let (urls, client, resolver) = self.routing.as_ref().map_or((&empty, None, None), |r| {
            (&r.subgraph_urls, Some(&r.http_client), r.entity_resolver.as_ref())
        });
        saga_executor.execute_saga(saga.id, executor, urls, client, resolver).await?;
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
