//! Automatic failover management for multi-listener setup.
//!
//! Detects listener failures and triggers automatic failover to healthy listeners,
//! maintaining checkpoint consistency and preventing data loss.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use tokio::sync::mpsc;

use super::{coordinator::MultiListenerCoordinator, state::ListenerState};
use crate::error::{ObserverError, Result};

/// Event triggered when a failover occurs
#[derive(Debug, Clone)]
pub struct FailoverEvent {
    /// ID of the listener that failed
    pub failed_listener_id: String,
    /// ID of the listener taking over (new leader)
    pub failover_target_id: String,
    /// Last processed checkpoint for recovery
    pub checkpoint: i64,
    /// When the failover was triggered
    pub timestamp: Instant,
}

/// Manages automatic failover between listeners
#[derive(Clone)]
pub struct FailoverManager {
    coordinator: Arc<MultiListenerCoordinator>,
    health_check_interval_ms: u64,
    failover_threshold_ms: u64,
    /// Shutdown signal for the health monitor task.
    shutdown: Arc<AtomicBool>,
}

impl FailoverManager {
    /// Create a new failover manager
    #[must_use]
    pub fn new(coordinator: Arc<MultiListenerCoordinator>) -> Self {
        Self {
            coordinator,
            health_check_interval_ms: 5000,
            failover_threshold_ms: 60_000,
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Create with custom intervals
    #[must_use]
    pub fn with_intervals(
        coordinator: Arc<MultiListenerCoordinator>,
        health_check_interval_ms: u64,
        failover_threshold_ms: u64,
    ) -> Self {
        Self {
            coordinator,
            health_check_interval_ms,
            failover_threshold_ms,
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Detect failed listeners using this manager's `failover_threshold_ms`.
    ///
    /// A listener is considered failed if it is not in `Running` state or its
    /// last heartbeat is older than `failover_threshold_ms`.
    ///
    /// # Errors
    ///
    /// Propagates errors from [`MultiListenerCoordinator::check_listener_health`].
    pub async fn detect_failures(&self) -> Result<Vec<String>> {
        let health = self.coordinator.check_listener_health().await?;
        let threshold = Duration::from_millis(self.failover_threshold_ms);
        let failed = health
            .into_iter()
            .filter(|h| {
                h.state != ListenerState::Running || h.last_heartbeat.elapsed() >= threshold
            })
            .map(|h| h.listener_id)
            .collect();
        Ok(failed)
    }

    /// Trigger failover from a failed listener to a healthy one
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::InvalidConfig`] if the failed listener is not registered.
    /// Propagates errors from [`MultiListenerCoordinator::check_listener_health`] and
    /// [`MultiListenerCoordinator::elect_leader`].
    pub async fn trigger_failover(&self, failed_listener_id: &str) -> Result<FailoverEvent> {
        // Get checkpoint from failed listener
        let checkpoint = self
            .coordinator
            .check_listener_health()
            .await?
            .iter()
            .find(|h| h.listener_id == failed_listener_id)
            .map(|h| h.last_checkpoint)
            .ok_or(ObserverError::InvalidConfig {
                message: format!("Listener {failed_listener_id} not found"),
            })?;

        // Elect new leader (failover target)
        let failover_target = self.coordinator.elect_leader().await?;

        Ok(FailoverEvent {
            failed_listener_id: failed_listener_id.to_string(),
            failover_target_id: failover_target,
            checkpoint,
            timestamp: Instant::now(),
        })
    }

    /// Resume processing from a checkpoint
    ///
    /// # Errors
    ///
    /// Propagates errors from [`MultiListenerCoordinator::update_checkpoint`] and
    /// [`MultiListenerCoordinator::transition_listener_state`].
    pub async fn resume_from_checkpoint(&self, listener_id: &str, checkpoint: i64) -> Result<()> {
        // Get listener and update checkpoint
        self.coordinator.update_checkpoint(listener_id, checkpoint)?;

        // Transition listener to Running state (if in Recovering)
        if let Ok(state) = self.coordinator.get_listener_state(listener_id).await {
            if state == ListenerState::Recovering {
                self.coordinator
                    .transition_listener_state(listener_id, ListenerState::Running)
                    .await?;
            }
        }

        Ok(())
    }

    /// Start the health monitoring loop.
    ///
    /// Spawns a background task that periodically checks for failed listeners
    /// and emits [`FailoverEvent`]s on the returned channel. The task exits
    /// when [`stop_health_monitor`] is called or the returned receiver is
    /// dropped.
    ///
    /// [`stop_health_monitor`]: Self::stop_health_monitor
    #[must_use]
    pub fn start_health_monitor(&self) -> mpsc::Receiver<FailoverEvent> {
        // Reset the shutdown flag so the monitor can be restarted.
        self.shutdown.store(false, Ordering::SeqCst);

        let (tx, rx) = mpsc::channel(100);
        let manager = self.clone();

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(manager.health_check_interval_ms)).await;

                // Exit if shutdown was requested.
                if manager.shutdown.load(Ordering::SeqCst) {
                    break;
                }

                if let Ok(failed_listeners) = manager.detect_failures().await {
                    for failed_id in failed_listeners {
                        if let Ok(event) = manager.trigger_failover(&failed_id).await {
                            // Exit if the receiver has been dropped.
                            if tx.send(event).await.is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        });

        rx
    }

    /// Stop the health monitoring task.
    ///
    /// Sets the shutdown flag; the spawned task will exit after its next
    /// sleep interval. Returns immediately — the task may still be running
    /// briefly after this call returns.
    pub fn stop_health_monitor(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    /// Get health check interval
    #[must_use]
    pub const fn health_check_interval_ms(&self) -> u64 {
        self.health_check_interval_ms
    }

    /// Get failover threshold
    #[must_use]
    pub const fn failover_threshold_ms(&self) -> u64 {
        self.failover_threshold_ms
    }
}
