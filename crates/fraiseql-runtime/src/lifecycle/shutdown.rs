use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::{broadcast, watch, Notify};
use tokio::time::timeout;

/// Coordinates graceful shutdown across all components
pub struct ShutdownCoordinator {
    /// Signal that shutdown has been initiated
    shutdown_initiated: AtomicBool,

    /// Sender for shutdown notification
    shutdown_tx: broadcast::Sender<()>,

    /// Watch channel for readiness state
    ready_tx: watch::Sender<bool>,
    ready_rx: watch::Receiver<bool>,

    /// Count of in-flight requests
    in_flight: AtomicU64,

    /// Notification when all requests complete
    drain_complete: Notify,

    /// Configuration
    config: ShutdownConfig,
}

#[derive(Debug, Clone)]
pub struct ShutdownConfig {
    /// Time to wait for in-flight requests to complete
    pub timeout: Duration,

    /// Delay before starting shutdown (for LB deregistration)
    pub delay: Duration,
}

impl Default for ShutdownConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            delay: Duration::from_secs(5),
        }
    }
}

impl ShutdownCoordinator {
    pub fn new(config: ShutdownConfig) -> Arc<Self> {
        let (shutdown_tx, _) = broadcast::channel(1);
        let (ready_tx, ready_rx) = watch::channel(true);

        Arc::new(Self {
            shutdown_initiated: AtomicBool::new(false),
            shutdown_tx,
            ready_tx,
            ready_rx,
            in_flight: AtomicU64::new(0),
            drain_complete: Notify::new(),
            config,
        })
    }

    /// Subscribe to shutdown signal
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    /// Get readiness watch receiver
    pub fn ready_watch(&self) -> watch::Receiver<bool> {
        self.ready_rx.clone()
    }

    /// Check if system is ready to accept requests
    pub fn is_ready(&self) -> bool {
        *self.ready_rx.borrow()
    }

    /// Check if shutdown has been initiated
    pub fn is_shutting_down(&self) -> bool {
        self.shutdown_initiated.load(Ordering::SeqCst)
    }

    /// Track a new in-flight request
    pub fn request_started(&self) -> Option<RequestGuard> {
        if self.is_shutting_down() {
            return None;
        }

        self.in_flight.fetch_add(1, Ordering::SeqCst);
        Some(RequestGuard { coordinator: self })
    }

    /// Get current in-flight request count
    pub fn in_flight_count(&self) -> u64 {
        self.in_flight.load(Ordering::SeqCst)
    }

    fn request_completed(&self) {
        let prev = self.in_flight.fetch_sub(1, Ordering::SeqCst);
        if prev == 1 && self.is_shutting_down() {
            self.drain_complete.notify_waiters();
        }
    }

    /// Initiate graceful shutdown
    pub async fn shutdown(&self) {
        if self.shutdown_initiated.swap(true, Ordering::SeqCst) {
            // Already shutting down
            return;
        }

        tracing::info!("Initiating graceful shutdown");

        // Step 1: Mark as not ready (stop accepting new requests)
        let _ = self.ready_tx.send(false);
        tracing::info!("Marked as not ready, waiting for load balancer deregistration");

        // Step 2: Wait for load balancer deregistration delay
        tokio::time::sleep(self.config.delay).await;

        // Step 3: Notify all components to shut down
        let _ = self.shutdown_tx.send(());
        tracing::info!("Shutdown signal sent to all components");

        // Step 4: Wait for in-flight requests to complete (with timeout)
        let in_flight = self.in_flight.load(Ordering::SeqCst);
        if in_flight > 0 {
            tracing::info!("Waiting for {} in-flight requests to complete", in_flight);

            let drain_result = timeout(
                self.config.timeout,
                self.wait_for_drain()
            ).await;

            match drain_result {
                Ok(()) => {
                    tracing::info!("All in-flight requests completed");
                }
                Err(_) => {
                    let remaining = self.in_flight.load(Ordering::SeqCst);
                    tracing::warn!(
                        "Shutdown timeout reached with {} requests still in-flight",
                        remaining
                    );
                }
            }
        }

        tracing::info!("Graceful shutdown complete");
    }

    async fn wait_for_drain(&self) {
        while self.in_flight.load(Ordering::SeqCst) > 0 {
            self.drain_complete.notified().await;
        }
    }
}

/// RAII guard for tracking in-flight requests
pub struct RequestGuard<'a> {
    coordinator: &'a ShutdownCoordinator,
}

impl Drop for RequestGuard<'_> {
    fn drop(&mut self) {
        self.coordinator.request_completed();
    }
}

/// Create shutdown signal future from OS signals
pub async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received Ctrl+C signal");
        }
        _ = terminate => {
            tracing::info!("Received SIGTERM signal");
        }
    }
}
