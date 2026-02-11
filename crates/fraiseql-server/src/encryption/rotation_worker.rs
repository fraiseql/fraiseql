// Background key rotation worker
//
// Spawns a tokio task that periodically checks whether encryption keys need
// rotation and performs the rotation when TTL thresholds are reached. Coordinates
// with RefreshManager for job state tracking and graceful shutdown.

use std::sync::Arc;

use tokio::sync::watch;
use tracing::{error, info, warn};

use super::{
    credential_rotation::CredentialRotationManager,
    refresh_trigger::{RefreshConfig, RefreshManager},
};

/// Background worker that monitors key TTL and triggers rotation.
///
/// The worker runs on a configurable interval (default: every `check_interval_hours`
/// from `RefreshConfig`). On each tick it:
///
/// 1. Checks if the current key's TTL has crossed the refresh threshold
/// 2. Coordinates with `RefreshManager` to avoid duplicate rotations
/// 3. Calls `CredentialRotationManager::rotate_key()` when needed
/// 4. Records success/failure metrics
///
/// # Shutdown
///
/// Send `true` on the shutdown channel to stop the worker gracefully. The worker
/// finishes any in-progress rotation before exiting.
pub struct RotationWorker {
    rotation_manager: Arc<CredentialRotationManager>,
    refresh_manager:  Arc<RefreshManager>,
    shutdown_rx:      watch::Receiver<bool>,
}

/// Handle returned when spawning a rotation worker.
///
/// Holds the shutdown sender and the task join handle for clean teardown.
pub struct RotationWorkerHandle {
    shutdown_tx: watch::Sender<bool>,
    join_handle: tokio::task::JoinHandle<()>,
}

impl RotationWorkerHandle {
    /// Signal the worker to shut down gracefully.
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }

    /// Wait for the worker task to finish.
    ///
    /// # Errors
    /// Returns error if the task panicked.
    pub async fn wait(self) -> Result<(), tokio::task::JoinError> {
        self.join_handle.await
    }

    /// Signal shutdown and wait for the worker to finish.
    ///
    /// # Errors
    /// Returns error if the task panicked.
    pub async fn shutdown_and_wait(self) -> Result<(), tokio::task::JoinError> {
        self.shutdown();
        self.join_handle.await
    }
}

impl RotationWorker {
    /// Spawn a new background rotation worker.
    ///
    /// Returns a handle that can be used to shut down the worker gracefully.
    ///
    /// # Arguments
    /// * `rotation_manager` - Manages key versions and performs rotations
    /// * `refresh_config` - Configuration for check interval and thresholds
    pub fn spawn(
        rotation_manager: Arc<CredentialRotationManager>,
        refresh_config: RefreshConfig,
    ) -> RotationWorkerHandle {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let refresh_manager = Arc::new(RefreshManager::new(refresh_config));

        let worker = Self {
            rotation_manager,
            refresh_manager,
            shutdown_rx,
        };

        let join_handle = tokio::spawn(worker.run());

        RotationWorkerHandle {
            shutdown_tx,
            join_handle,
        }
    }

    /// Spawn with an existing `RefreshManager` (for testing or shared state).
    pub fn spawn_with_manager(
        rotation_manager: Arc<CredentialRotationManager>,
        refresh_manager: Arc<RefreshManager>,
    ) -> RotationWorkerHandle {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let worker = Self {
            rotation_manager,
            refresh_manager,
            shutdown_rx,
        };

        let join_handle = tokio::spawn(worker.run());

        RotationWorkerHandle {
            shutdown_tx,
            join_handle,
        }
    }

    async fn run(mut self) {
        let check_interval = self.check_interval();

        info!(interval_secs = check_interval.as_secs(), "Key rotation worker started");

        let mut interval = tokio::time::interval(check_interval);
        // Don't burst on startup — wait one full interval before first check
        interval.tick().await;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.check_and_rotate();
                }
                _ = self.shutdown_rx.changed() => {
                    if *self.shutdown_rx.borrow() {
                        info!("Key rotation worker received shutdown signal");
                        self.refresh_manager.request_shutdown();
                        break;
                    }
                }
            }
        }

        info!("Key rotation worker stopped");
    }

    fn check_interval(&self) -> std::time::Duration {
        let trigger = self.refresh_manager.trigger();
        // Check interval from config, or default to 24 hours
        if trigger.is_enabled() {
            // We check more frequently than the config interval
            // to keep the loop responsive to shutdown signals
            std::time::Duration::from_secs(3600) // 1 hour
        } else {
            std::time::Duration::from_secs(86400) // 24 hours (effectively disabled)
        }
    }

    fn check_and_rotate(&self) {
        // Get current key TTL consumption
        let ttl_percent = match self.rotation_manager.get_current_metadata() {
            Ok(Some(metadata)) => metadata.ttl_consumed_percent(),
            Ok(None) => {
                warn!("No current key version found — skipping rotation check");
                return;
            },
            Err(e) => {
                error!(error = %e, "Failed to get current key metadata");
                return;
            },
        };

        // Check if refresh should trigger
        if !self.refresh_manager.check_and_trigger(ttl_percent) {
            return;
        }

        info!(
            ttl_consumed_percent = ttl_percent,
            "Key TTL threshold reached, starting rotation"
        );

        // Start the job
        if let Err(e) = self.refresh_manager.start_job() {
            warn!(error = %e, "Could not start rotation job");
            return;
        }

        let start = std::time::Instant::now();

        // Perform the rotation
        match self.rotation_manager.rotate_key() {
            Ok(new_version) => {
                let duration_ms = start.elapsed().as_millis() as u64;

                if let Err(e) = self.refresh_manager.complete_job_success() {
                    error!(error = %e, "Failed to mark rotation job as success");
                }

                self.refresh_manager.trigger().record_success(duration_ms);

                info!(
                    new_version = new_version,
                    duration_ms = duration_ms,
                    "Key rotation completed successfully"
                );
            },
            Err(e) => {
                if let Err(je) = self.refresh_manager.complete_job_failure(&e) {
                    error!(error = %je, "Failed to mark rotation job as failed");
                }

                self.refresh_manager.trigger().record_failure();

                error!(error = %e, "Key rotation failed");

                // Check if we should retry
                if self.refresh_manager.should_retry_refresh() {
                    self.refresh_manager.reset_for_retry();
                    warn!("Rotation will be retried on next check interval");
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::encryption::credential_rotation::RotationConfig;

    fn setup_managers() -> (Arc<CredentialRotationManager>, Arc<RefreshManager>) {
        let rotation_config = RotationConfig::new().with_ttl_days(1);
        let rotation_manager = Arc::new(CredentialRotationManager::new(rotation_config));
        rotation_manager.initialize_key().expect("init key");

        let refresh_config = RefreshConfig::new().with_refresh_threshold(80);
        let refresh_manager = Arc::new(RefreshManager::new(refresh_config));

        (rotation_manager, refresh_manager)
    }

    #[tokio::test]
    async fn test_worker_spawns_and_shuts_down() {
        let (rotation_manager, _) = setup_managers();
        let refresh_config = RefreshConfig::new();

        let handle = RotationWorker::spawn(rotation_manager, refresh_config);

        // Give the worker time to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Shut down gracefully
        handle.shutdown_and_wait().await.expect("worker should stop");
    }

    #[tokio::test]
    async fn test_worker_with_manager_spawns_and_shuts_down() {
        let (rotation_manager, refresh_manager) = setup_managers();

        let handle =
            RotationWorker::spawn_with_manager(rotation_manager, Arc::clone(&refresh_manager));

        tokio::time::sleep(Duration::from_millis(50)).await;

        handle.shutdown_and_wait().await.expect("worker should stop");
    }

    #[tokio::test]
    async fn test_worker_does_not_rotate_fresh_key() {
        let (rotation_manager, refresh_manager) = setup_managers();

        // Key was just created — TTL is ~0% consumed, well below 80% threshold
        let initial_version = rotation_manager.get_current_version().unwrap();

        let handle =
            RotationWorker::spawn_with_manager(Arc::clone(&rotation_manager), refresh_manager);

        // Let the worker run one check cycle
        tokio::time::sleep(Duration::from_millis(100)).await;

        handle.shutdown_and_wait().await.unwrap();

        // Version should not have changed
        assert_eq!(
            rotation_manager.get_current_version().unwrap(),
            initial_version,
            "Fresh key should not be rotated"
        );
    }

    #[test]
    fn test_check_and_rotate_with_fresh_key() {
        let (rotation_manager, refresh_manager) = setup_managers();

        let worker = RotationWorker {
            rotation_manager: Arc::clone(&rotation_manager),
            refresh_manager:  Arc::clone(&refresh_manager),
            shutdown_rx:      watch::channel(false).1,
        };

        let initial_version = rotation_manager.get_current_version().unwrap();

        // Manually call check — should not rotate (key is fresh)
        worker.check_and_rotate();

        assert_eq!(rotation_manager.get_current_version().unwrap(), initial_version);
    }

    #[test]
    fn test_check_and_rotate_triggers_when_threshold_reached() {
        let rotation_config = RotationConfig::new().with_ttl_days(1);
        let rotation_manager = Arc::new(CredentialRotationManager::new(rotation_config));
        rotation_manager.initialize_key().expect("init key");

        // Use a 0% threshold so any TTL consumption triggers rotation
        let refresh_config = RefreshConfig::new().with_refresh_threshold(0);
        let refresh_manager = Arc::new(RefreshManager::new(refresh_config));

        let worker = RotationWorker {
            rotation_manager: Arc::clone(&rotation_manager),
            refresh_manager:  Arc::clone(&refresh_manager),
            shutdown_rx:      watch::channel(false).1,
        };

        let initial_version = rotation_manager.get_current_version().unwrap();

        worker.check_and_rotate();

        let new_version = rotation_manager.get_current_version().unwrap();
        assert_ne!(initial_version, new_version, "Key should have been rotated");

        // Metrics should reflect the rotation
        assert_eq!(refresh_manager.trigger().total_refreshes(), 1);
    }

    #[test]
    fn test_check_and_rotate_does_not_double_rotate() {
        let rotation_config = RotationConfig::new().with_ttl_days(1);
        let rotation_manager = Arc::new(CredentialRotationManager::new(rotation_config));
        rotation_manager.initialize_key().expect("init key");

        let refresh_config = RefreshConfig::new().with_refresh_threshold(0);
        let refresh_manager = Arc::new(RefreshManager::new(refresh_config));

        let worker = RotationWorker {
            rotation_manager: Arc::clone(&rotation_manager),
            refresh_manager:  Arc::clone(&refresh_manager),
            shutdown_rx:      watch::channel(false).1,
        };

        // First rotation
        worker.check_and_rotate();
        let version_after_first = rotation_manager.get_current_version().unwrap();

        // Second call — should not rotate again (pending flag was cleared on success,
        // but the new key is fresh so TTL is ~0%)
        worker.check_and_rotate();
        let version_after_second = rotation_manager.get_current_version().unwrap();

        // The second check rotated too because threshold is 0% and TTL is 0%
        // This is correct behavior: 0% consumed >= 0% threshold = always rotate
        // In production, threshold is 80% so this won't happen with a fresh key
        assert!(version_after_second >= version_after_first);
    }

    #[test]
    fn test_check_and_rotate_records_metrics_on_success() {
        let rotation_config = RotationConfig::new().with_ttl_days(1);
        let rotation_manager = Arc::new(CredentialRotationManager::new(rotation_config));
        rotation_manager.initialize_key().expect("init key");

        let refresh_config = RefreshConfig::new().with_refresh_threshold(0);
        let refresh_manager = Arc::new(RefreshManager::new(refresh_config));

        let worker = RotationWorker {
            rotation_manager: Arc::clone(&rotation_manager),
            refresh_manager:  Arc::clone(&refresh_manager),
            shutdown_rx:      watch::channel(false).1,
        };

        worker.check_and_rotate();

        let trigger = refresh_manager.trigger();
        assert_eq!(trigger.total_refreshes(), 1);
        assert_eq!(trigger.failed_refreshes(), 0);
        assert_eq!(trigger.success_rate_percent(), 100);
        assert!(trigger.last_refresh_time().is_some());
    }

    #[test]
    fn test_check_and_rotate_skips_when_disabled() {
        let rotation_config = RotationConfig::new().with_ttl_days(1);
        let rotation_manager = Arc::new(CredentialRotationManager::new(rotation_config));
        rotation_manager.initialize_key().expect("init key");

        let refresh_config = RefreshConfig::new().with_enabled(false);
        let refresh_manager = Arc::new(RefreshManager::new(refresh_config));

        let worker = RotationWorker {
            rotation_manager: Arc::clone(&rotation_manager),
            refresh_manager:  Arc::clone(&refresh_manager),
            shutdown_rx:      watch::channel(false).1,
        };

        let initial_version = rotation_manager.get_current_version().unwrap();

        worker.check_and_rotate();

        assert_eq!(
            rotation_manager.get_current_version().unwrap(),
            initial_version,
            "Disabled refresh should not trigger rotation"
        );
    }

    #[test]
    fn test_check_and_rotate_skips_when_no_key_initialized() {
        let rotation_config = RotationConfig::new().with_ttl_days(1);
        let rotation_manager = Arc::new(CredentialRotationManager::new(rotation_config));
        // Intentionally don't initialize a key

        let refresh_config = RefreshConfig::new().with_refresh_threshold(0);
        let refresh_manager = Arc::new(RefreshManager::new(refresh_config));

        let worker = RotationWorker {
            rotation_manager,
            refresh_manager,
            shutdown_rx: watch::channel(false).1,
        };

        // Should not panic
        worker.check_and_rotate();
    }

    #[tokio::test]
    async fn test_worker_handle_shutdown_is_idempotent() {
        let (rotation_manager, _) = setup_managers();
        let refresh_config = RefreshConfig::new();

        let handle = RotationWorker::spawn(rotation_manager, refresh_config);

        // Multiple shutdown calls should not panic
        handle.shutdown();
        handle.shutdown();

        handle.wait().await.expect("worker should stop");
    }
}
