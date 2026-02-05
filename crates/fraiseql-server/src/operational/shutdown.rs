//! Graceful shutdown handling
//!
//! Manages signal handling and graceful server shutdown

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Shutdown handler
#[derive(Clone)]
pub struct ShutdownHandler {
    shutdown_requested: Arc<AtomicBool>,
    in_flight_requests: Arc<atomic::AtomicU32>,
}

impl ShutdownHandler {
    /// Create new shutdown handler
    pub fn new() -> Self {
        Self {
            shutdown_requested: Arc::new(AtomicBool::new(false)),
            in_flight_requests: Arc::new(atomic::AtomicU32::new(0)),
        }
    }

    /// Signal shutdown request
    pub fn request_shutdown(&self) {
        self.shutdown_requested.store(true, Ordering::Release);
    }

    /// Check if shutdown is requested
    pub fn is_shutdown_requested(&self) -> bool {
        self.shutdown_requested.load(Ordering::Acquire)
    }

    /// Increment in-flight request counter
    pub fn increment_requests(&self) {
        let _ = self.in_flight_requests.fetch_add(1, Ordering::AcqRel);
    }

    /// Decrement in-flight request counter
    pub fn decrement_requests(&self) {
        let _ = self.in_flight_requests.fetch_sub(1, Ordering::AcqRel);
    }

    /// Get number of in-flight requests
    pub fn in_flight_count(&self) -> u32 {
        self.in_flight_requests.load(Ordering::Acquire)
    }

    /// Wait for all requests to complete
    pub async fn wait_for_requests(&self) {
        while self.in_flight_count() > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }
}

impl Default for ShutdownHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Install signal handlers for graceful shutdown
pub async fn install_signal_handlers(handler: ShutdownHandler) -> Result<(), std::io::Error> {
    use tokio::signal;

    let handler_clone = handler.clone();
    tokio::spawn(async move {
        let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to create SIGTERM handler");

        sigterm.recv().await;
        handler_clone.request_shutdown();
    });

    let handler_clone = handler.clone();
    tokio::spawn(async move {
        let _ = signal::ctrl_c().await;
        handler_clone.request_shutdown();
    });

    Ok(())
}

// Use atomic module re-export
use std::sync::atomic;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shutdown_handler() {
        let handler = ShutdownHandler::new();
        assert!(!handler.is_shutdown_requested());

        handler.request_shutdown();
        assert!(handler.is_shutdown_requested());
    }

    #[test]
    fn test_request_counting() {
        let handler = ShutdownHandler::new();
        assert_eq!(handler.in_flight_count(), 0);

        handler.increment_requests();
        handler.increment_requests();
        assert_eq!(handler.in_flight_count(), 2);

        handler.decrement_requests();
        assert_eq!(handler.in_flight_count(), 1);
    }
}
