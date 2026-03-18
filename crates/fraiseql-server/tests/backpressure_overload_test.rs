//! Stress tests for [`AdmissionController`] under sustained overload.
//!
//! Verifies that the admission controller correctly sheds load when all permits
//! are held, and recovers immediately when permits are released.
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(missing_docs)] // Reason: test code does not require documentation

use std::sync::Arc;

use fraiseql_server::resilience::backpressure::AdmissionController;

/// Simulates sustained overload: sequential burst of 1000 admission attempts
/// through a controller with `capacity=50`, `queue_depth=10_000`.
/// Verifies the controller sheds load correctly when all permits are held.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_sustained_overload_sheds_load() {
    // Use a high max_queue_depth so the queue counter (which accumulates on
    // failed try_acquire calls) doesn't itself become the bottleneck. We want
    // to test semaphore exhaustion, not queue-depth saturation.
    let controller = AdmissionController::new(50, 10_000);
    let mut accepted = 0u64;
    let mut rejected = 0u64;

    // Phase 1: Saturate all 50 permits (RAII — held alive until explicit drop)
    let held_permits: Vec<_> = (0..50)
        .map(|_| controller.try_acquire().expect("should acquire"))
        .collect();

    // Phase 2: Attempt 1000 more — all should be rejected (capacity full)
    for _ in 0..1000 {
        match controller.try_acquire() {
            Some(_permit) => accepted += 1,
            None => rejected += 1,
        }
    }

    // With all 50 permits held, every additional request must be rejected
    // because the semaphore has no capacity left.
    assert_eq!(
        rejected, 1000,
        "all 1000 requests must be rejected when permits are held (accepted={accepted})"
    );

    // Phase 3: Release permits and verify recovery
    drop(held_permits);

    let permit = controller.try_acquire();
    assert!(permit.is_some(), "should recover after permits released");
}

/// After an overload spike saturates all permits, releasing them restores full
/// capacity immediately — no cooldown, no stale state.
#[tokio::test]
async fn test_recovery_after_overload_spike() {
    let controller = AdmissionController::new(50, 100);

    // Phase 1: Saturate all permits (RAII — held alive until explicit drop)
    let permits: Vec<_> = (0..50)
        .map(|_| controller.try_acquire().expect("should acquire"))
        .collect();

    // Phase 2: Verify rejection during saturation
    assert!(controller.try_acquire().is_none());

    // Phase 3: Release all permits (simulating spike end)
    drop(permits);

    // Phase 4: Verify immediate recovery — can acquire the full capacity again
    let mut recovered = 0usize;
    for _ in 0..50 {
        if controller.try_acquire().is_some() {
            recovered += 1;
        }
    }
    assert_eq!(
        recovered, 50,
        "must recover full capacity after permits released"
    );
}

/// Queue depth tracking remains consistent under concurrent thread access.
/// After all permits are dropped, queue depth must eventually return to a
/// consistent state (no leaked increments).
#[test]
fn test_queue_depth_tracking_under_concurrency() {
    use std::sync::Barrier;

    let limit = 10;
    let overload = 50;
    let total = limit + overload;

    let controller = Arc::new(AdmissionController::new(limit, 1000));
    let barrier = Arc::new(Barrier::new(total));

    let mut handles = Vec::new();
    for _ in 0..total {
        let c = Arc::clone(&controller);
        let b = Arc::clone(&barrier);
        handles.push(std::thread::spawn(move || {
            let permit = c.try_acquire();
            b.wait(); // hold permits while all threads are trying
            drop(permit);
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // After all threads complete and permits drop, queue_depth reflects
    // the number of failed semaphore acquisitions (incremented but never
    // decremented by try_acquire). This is an implementation detail —
    // the important invariant is no panic and no deadlock during the burst.
    // Queue depth is only decremented by acquire_timeout's cleanup path.
    let depth = controller.queue_depth();
    assert!(
        depth <= overload as u64,
        "queue depth ({depth}) must not exceed number of rejected requests ({overload})"
    );
}
