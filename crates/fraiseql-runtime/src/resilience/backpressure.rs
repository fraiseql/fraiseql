use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::Semaphore;
use std::time::Duration;

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
    pub fn new(max_concurrent: usize, max_queue_depth: u64) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            queue_depth: AtomicU64::new(0),
            max_queue_depth,
        }
    }

    /// Try to acquire a permit without blocking
    pub fn try_acquire(&self) -> Option<AdmissionPermit<'static>> {
        // Check queue depth first
        let current_depth = self.queue_depth.load(Ordering::Relaxed);
        if current_depth >= self.max_queue_depth {
            return None;
        }

        // Try to acquire permit
        match self.semaphore.clone().try_acquire_owned() {
            Ok(permit) => Some(AdmissionPermit {
                _permit: permit,
                controller: self as *const Self as usize,
                _phantom: std::marker::PhantomData,
            }),
            Err(_) => {
                // No permits available, increment queue depth
                self.queue_depth.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// Acquire a permit with timeout
    pub async fn acquire_timeout(&self, timeout: Duration) -> Option<AdmissionPermit<'static>> {
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
                    _permit: permit,
                    controller: self as *const Self as usize,
                    _phantom: std::marker::PhantomData,
                })
            }
            _ => {
                self.queue_depth.fetch_sub(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// Get current queue depth
    pub fn queue_depth(&self) -> u64 {
        self.queue_depth.load(Ordering::Relaxed)
    }
}

/// RAII guard that holds an admission permit
pub struct AdmissionPermit<'a> {
    _permit: tokio::sync::OwnedSemaphorePermit,
    controller: usize, // Pointer to controller (for lifetime hack)
    _phantom: std::marker::PhantomData<&'a ()>,
}

// Safety: This is safe because we never actually dereference the controller pointer
// It's only used to tie the lifetime to the controller
impl<'a> AdmissionPermit<'a> {
    fn _new(_permit: tokio::sync::OwnedSemaphorePermit, controller: usize) -> Self {
        Self {
            _permit,
            controller,
            _phantom: std::marker::PhantomData,
        }
    }
}

// Manual implementation to avoid phantom data issues
impl Drop for AdmissionPermit<'_> {
    fn drop(&mut self) {
        // Permit is automatically released by drop
    }
}
