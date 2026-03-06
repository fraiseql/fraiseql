//! Admission control and backpressure for the FraiseQL server.
//!
//! [`AdmissionController`] enforces a concurrent-request limit using a
//! semaphore and rejects requests that would exceed the configured queue
//! depth, returning `503 Service Unavailable` instead of stalling.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use tokio::sync::Semaphore;

/// Admission controller for managing concurrency and backpressure
pub struct AdmissionController {
    /// Semaphore limiting concurrent requests
    semaphore: Arc<Semaphore>,

    /// Current queue depth (waiting requests)
    queue_depth: AtomicU64,

    /// Maximum allowed queue depth
    max_queue_depth: u64,
}

impl AdmissionController {
    /// Create a new `AdmissionController` with the given concurrency and queue limits.
    pub fn new(max_concurrent: usize, max_queue_depth: u64) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            queue_depth: AtomicU64::new(0),
            max_queue_depth,
        }
    }

    /// Try to acquire a permit without blocking.
    ///
    /// Returns `None` if the queue is full or no permits are available.
    /// The permit borrows from `self`, so it cannot outlive the controller.
    pub fn try_acquire(&self) -> Option<AdmissionPermit<'_>> {
        // Check queue depth first
        let current_depth = self.queue_depth.load(Ordering::Relaxed);
        if current_depth >= self.max_queue_depth {
            return None;
        }

        // Try to acquire permit
        match self.semaphore.clone().try_acquire_owned() {
            Ok(permit) => Some(AdmissionPermit {
                _permit:  permit,
                _phantom: std::marker::PhantomData,
            }),
            Err(_) => {
                // No permits available, increment queue depth
                self.queue_depth.fetch_add(1, Ordering::Relaxed);
                None
            },
        }
    }

    /// Acquire a permit with timeout.
    ///
    /// Returns `None` if the queue is full or the timeout elapses.
    /// The permit borrows from `self`, so it cannot outlive the controller.
    pub async fn acquire_timeout(&self, timeout: Duration) -> Option<AdmissionPermit<'_>> {
        // Check queue depth
        let current_depth = self.queue_depth.load(Ordering::Relaxed);
        if current_depth >= self.max_queue_depth {
            return None;
        }

        // Increment queue depth while waiting
        self.queue_depth.fetch_add(1, Ordering::Relaxed);

        // Try to acquire with timeout
        match tokio::time::timeout(timeout, self.semaphore.clone().acquire_owned()).await {
            Ok(Ok(permit)) => {
                self.queue_depth.fetch_sub(1, Ordering::Relaxed);
                Some(AdmissionPermit {
                    _permit:  permit,
                    _phantom: std::marker::PhantomData,
                })
            },
            _ => {
                self.queue_depth.fetch_sub(1, Ordering::Relaxed);
                None
            },
        }
    }

    /// Get current queue depth
    pub fn queue_depth(&self) -> u64 {
        self.queue_depth.load(Ordering::Relaxed)
    }
}

/// RAII guard that holds an admission permit.
///
/// The `'a` lifetime is bound to the [`AdmissionController`] that issued the
/// permit, preventing the permit from outliving the controller.
pub struct AdmissionPermit<'a> {
    _permit:  tokio::sync::OwnedSemaphorePermit,
    /// Binds the permit lifetime to the issuing `AdmissionController`.
    _phantom: std::marker::PhantomData<&'a AdmissionController>,
}

// Manual implementation to avoid phantom data issues
impl Drop for AdmissionPermit<'_> {
    fn drop(&mut self) {
        // Permit is automatically released by drop
    }
}
