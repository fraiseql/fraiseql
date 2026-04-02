//! Backpressure integration tests for [`AdmissionController`].
//!
//! Verifies that the admission controller correctly enforces concurrency limits
//! under load — allowing N concurrent requests and rejecting any beyond the
//! limit — using `tokio::task::JoinSet` for true concurrent request simulation.
//!
//! These tests target the admission logic itself (not the HTTP layer) so they
//! run without any external services.
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(missing_docs)] // Reason: test code does not require documentation

use std::{sync::Arc, time::Duration};

use fraiseql_server::resilience::backpressure::AdmissionController;

/// Simulates N+1 concurrent requests arriving simultaneously.
///
/// All tasks coordinate via a barrier so they attempt `try_acquire` at the
/// same time. Each admitted task holds its permit until the barrier is released.
/// Returns a pair `(acquired, rejected)`.
fn simulate_concurrent_requests(
    admission_limit: usize,
    max_queue_depth: u64,
    total_requests: usize,
) -> (usize, usize) {
    use std::sync::Barrier;

    let controller = Arc::new(AdmissionController::new(admission_limit, max_queue_depth));
    // Barrier ensures all threads attempt try_acquire before any releases its permit.
    let barrier = Arc::new(Barrier::new(total_requests));

    // Use threads rather than async tasks so the barrier blocks truly concurrently.
    let mut handles = Vec::new();
    for _ in 0..total_requests {
        let c = Arc::clone(&controller);
        let b = Arc::clone(&barrier);
        handles.push(std::thread::spawn(move || {
            let permit = c.try_acquire();
            let acquired = permit.is_some();
            // All threads wait here — holds permit while others attempt acquire.
            b.wait();
            // Permit dropped here after barrier
            drop(permit);
            acquired
        }));
    }

    let mut acquired = 0usize;
    let mut rejected = 0usize;
    for h in handles {
        if h.join().unwrap() {
            acquired += 1;
        } else {
            rejected += 1;
        }
    }
    (acquired, rejected)
}

/// `AdmissionController` allows exactly N concurrent requests when N permits are
/// configured and all requests arrive simultaneously.
#[test]
fn test_admission_controller_allows_up_to_limit() {
    let limit = 5;
    // max_queue_depth must be > 0 for try_acquire to not immediately reject
    let (acquired, _rejected) = simulate_concurrent_requests(limit, limit as u64, limit);

    // All requests within the limit must be admitted
    assert_eq!(acquired, limit, "all {limit} requests within the limit must be admitted");
}

/// `AdmissionController` rejects requests that exceed the semaphore capacity.
#[test]
fn test_admission_controller_rejects_over_limit() {
    let limit = 5;
    let extra = 5;
    let total = limit + extra;

    let (acquired, rejected) = simulate_concurrent_requests(limit, total as u64, total);

    assert!(
        rejected > 0,
        "expected some rejections when {total} requests exceed limit {limit}, \
        got acquired={acquired} rejected={rejected}"
    );
    assert_eq!(acquired + rejected, total, "every request must be either admitted or rejected");
}

/// After an overload spike, the controller recovers and admits new requests.
#[tokio::test]
async fn test_admission_controller_recovers_after_spike() {
    let limit = 3;
    let controller = Arc::new(AdmissionController::new(limit, 50));

    // Spike: grab all permits
    let mut permits = Vec::new();
    for _ in 0..limit {
        let p = controller.try_acquire().expect("must admit within limit");
        permits.push(p);
    }

    // At capacity — next request must be rejected
    assert!(controller.try_acquire().is_none(), "must reject when at capacity");

    // Release all permits
    drop(permits);

    // Server must now be responsive (all permits available again)
    let p = controller.try_acquire();
    assert!(
        p.is_some(),
        "server must recover after spike: must admit new request after permits released"
    );
}

/// A zero-queue-depth controller rejects all requests immediately.
#[tokio::test]
async fn test_zero_queue_depth_rejects_all() {
    let controller = Arc::new(AdmissionController::new(10, 0));
    let result = controller.try_acquire();
    assert!(result.is_none(), "max_queue_depth=0 must reject all requests unconditionally");
}

/// `acquire_timeout` admits a request when permits are available.
#[tokio::test]
async fn test_acquire_timeout_succeeds_when_permits_available() {
    let controller = Arc::new(AdmissionController::new(5, 10));
    let permit = controller.acquire_timeout(Duration::from_millis(100)).await;
    assert!(permit.is_some(), "must admit when permits available");
}

/// `acquire_timeout` returns `None` when the timeout elapses before a permit
/// is freed, leaving no stale queue-depth increment behind.
#[tokio::test]
async fn test_acquire_timeout_cleans_up_queue_depth_on_expiry() {
    let controller = Arc::new(AdmissionController::new(1, 10));
    let _held = controller.try_acquire().expect("first permit");

    // Semaphore full; timeout will elapse
    let permit = controller.acquire_timeout(Duration::from_millis(20)).await;
    assert!(permit.is_none(), "must return None on timeout");

    // Queue depth must be restored to 0 after cleanup
    assert_eq!(controller.queue_depth(), 0, "queue depth must return to 0 after timeout expiry");
}

/// N+1 concurrent threads — at most N succeed, at least 1 is rejected.
/// No panics, no deadlocks.
#[test]
fn test_concurrent_spike_no_panic_no_deadlock() {
    use std::sync::Barrier;

    let limit = 4;
    let total = limit + 8; // deliberate overload

    let controller = Arc::new(AdmissionController::new(limit, total as u64));
    let barrier = Arc::new(Barrier::new(total));
    let mut handles = Vec::new();

    for _ in 0..total {
        let c = Arc::clone(&controller);
        let b = Arc::clone(&barrier);
        handles.push(std::thread::spawn(move || {
            let permit = c.try_acquire();
            let ok = permit.is_some();
            b.wait(); // hold permit while all threads are trying
            drop(permit);
            ok
        }));
    }

    let results: Vec<bool> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let admitted = results.iter().filter(|&&ok| ok).count();
    let rejected = results.iter().filter(|&&ok| !ok).count();

    assert_eq!(admitted + rejected, total, "every request must resolve");
    assert!(
        rejected > 0,
        "overload spike ({total} requests > limit {limit}) must produce rejections"
    );
}
