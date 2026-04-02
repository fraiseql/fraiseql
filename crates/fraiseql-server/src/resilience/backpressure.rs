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
        if let Ok(permit) = self.semaphore.clone().try_acquire_owned() {
            Some(AdmissionPermit {
                _permit:  permit,
                _phantom: std::marker::PhantomData,
            })
        } else {
            // No permits available, increment queue depth
            self.queue_depth.fetch_add(1, Ordering::Relaxed);
            None
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
        let result = tokio::time::timeout(timeout, self.semaphore.clone().acquire_owned()).await;
        self.queue_depth.fetch_sub(1, Ordering::Relaxed);
        if let Ok(Ok(permit)) = result {
            Some(AdmissionPermit {
                _permit:  permit,
                _phantom: std::marker::PhantomData,
            })
        } else {
            None
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_when_below_capacity() {
        let ac = AdmissionController::new(10, 100);
        let permit = ac.try_acquire();
        assert!(permit.is_some(), "must allow when below capacity");
    }

    #[test]
    fn rejects_when_semaphore_exhausted() {
        // Capacity 2 concurrent, queue depth 10
        let ac = AdmissionController::new(2, 10);
        let _p1 = ac.try_acquire().expect("1st permit");
        let _p2 = ac.try_acquire().expect("2nd permit");
        // 3rd: semaphore exhausted → rejected (and queue_depth incremented)
        assert!(ac.try_acquire().is_none(), "must reject when semaphore exhausted");
    }

    #[test]
    fn releases_on_permit_drop() {
        let ac = AdmissionController::new(1, 10);
        {
            let _p = ac.try_acquire().expect("must succeed");
            // At capacity — semaphore exhausted
            assert!(ac.try_acquire().is_none(), "at capacity: must reject");
        }
        // _p dropped — slot released
        assert!(ac.try_acquire().is_some(), "after permit drop, must allow new request");
    }

    #[test]
    fn queue_depth_tracked_on_semaphore_exhaustion() {
        let ac = AdmissionController::new(1, 10);
        let _p = ac.try_acquire().expect("first permit");
        assert_eq!(ac.queue_depth(), 0, "no queueing yet");

        // Second try_acquire: semaphore exhausted → queue_depth incremented, returns None
        assert!(ac.try_acquire().is_none());
        assert_eq!(ac.queue_depth(), 1, "queue_depth must be 1 after one failed acquire");
    }

    #[test]
    fn zero_max_queue_depth_rejects_all() {
        // max_queue_depth=0 means the queue check `0 >= 0` rejects immediately
        let ac = AdmissionController::new(10, 0);
        assert!(ac.try_acquire().is_none(), "max_queue_depth=0 must reject all requests");
    }

    #[tokio::test]
    async fn acquire_timeout_succeeds_when_available() {
        let ac = AdmissionController::new(5, 10);
        let permit = ac.acquire_timeout(Duration::from_millis(100)).await;
        assert!(permit.is_some(), "must succeed when permits available");
    }

    #[tokio::test]
    async fn acquire_timeout_rejects_when_queue_full() {
        // max_queue_depth=0 → immediate rejection at queue check
        let ac = AdmissionController::new(1, 0);
        let permit = ac.acquire_timeout(Duration::from_millis(10)).await;
        assert!(permit.is_none(), "must reject when max_queue_depth=0");
    }

    #[tokio::test]
    async fn acquire_timeout_returns_none_on_expiry() {
        let ac = AdmissionController::new(1, 10);
        let _p = ac.try_acquire().expect("first permit");
        // Semaphore exhausted, queue has space, but timeout will expire
        let permit = ac.acquire_timeout(Duration::from_millis(10)).await;
        assert!(permit.is_none(), "must return None when timeout elapses");
        // Queue depth must be decremented back to 0 after timeout
        assert_eq!(ac.queue_depth(), 0, "queue_depth must be 0 after timeout cleanup");
    }

    #[tokio::test]
    async fn acquire_timeout_succeeds_when_permit_freed_in_time() {
        let ac = AdmissionController::new(1, 10);
        let p = ac.try_acquire().expect("first permit");

        // Drop the permit after a short delay, then try acquire_timeout
        tokio::task::yield_now().await;
        drop(p);

        let result = ac.acquire_timeout(Duration::from_secs(1)).await;
        assert!(result.is_some(), "must succeed when permit freed before timeout");
    }
}
