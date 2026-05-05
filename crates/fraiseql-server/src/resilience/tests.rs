mod backpressure_tests {
    use std::time::Duration;

    use super::super::backpressure::AdmissionController;

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
