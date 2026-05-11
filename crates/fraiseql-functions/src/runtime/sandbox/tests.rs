#![allow(clippy::unwrap_used)] // Reason: test code

use super::*;

#[test]
fn test_limiter_allows_up_to_max_concurrent() {
    let limiter = ConcurrencyLimiter::new(3);
    let _p1 = limiter.try_acquire().unwrap();
    let _p2 = limiter.try_acquire().unwrap();
    let _p3 = limiter.try_acquire().unwrap();

    // 4th attempt must be rejected
    assert!(limiter.try_acquire().is_err());
}

#[test]
fn test_limiter_releases_permit_on_drop() {
    let limiter = ConcurrencyLimiter::new(1);

    {
        let _permit = limiter.try_acquire().unwrap();
        assert!(limiter.try_acquire().is_err()); // at capacity
    } // permit dropped here

    // Should be available again
    assert!(limiter.try_acquire().is_ok());
}

#[test]
fn test_limiter_error_message_includes_limit() {
    let limiter = ConcurrencyLimiter::new(2);
    let _p1 = limiter.try_acquire().unwrap();
    let _p2 = limiter.try_acquire().unwrap();

    let err = limiter.try_acquire().unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains('2') || msg.contains("concurrency"), "error: {msg}");
}

#[test]
fn test_limiter_available_permits_tracks_usage() {
    let limiter = ConcurrencyLimiter::new(4);
    assert_eq!(limiter.available_permits(), 4);

    let _p1 = limiter.try_acquire().unwrap();
    assert_eq!(limiter.available_permits(), 3);

    let _p2 = limiter.try_acquire().unwrap();
    assert_eq!(limiter.available_permits(), 2);
}

#[test]
fn test_registry_creates_limiters_on_demand() {
    let registry = ConcurrencyLimiterRegistry::new(5);
    let limiter = registry.get_or_create("my_function");
    assert_eq!(limiter.max_concurrent(), 5);
}

#[test]
fn test_registry_reuses_existing_limiters() {
    let registry = ConcurrencyLimiterRegistry::new(3);
    let l1 = registry.get_or_create("fn_a");
    let l2 = registry.get_or_create("fn_a");

    // Same Arc pointer (same semaphore state)
    assert!(Arc::ptr_eq(&l1.semaphore, &l2.semaphore));
}

#[test]
fn test_registry_isolates_different_functions() {
    let registry = ConcurrencyLimiterRegistry::new(1);
    let fn_a = registry.get_or_create("fn_a");
    let fn_b = registry.get_or_create("fn_b");

    let _permit_a = fn_a.try_acquire().unwrap();
    // fn_a is at capacity, but fn_b is independent
    assert!(fn_a.try_acquire().is_err());
    assert!(fn_b.try_acquire().is_ok());
}

#[test]
fn test_registry_custom_per_function_limit() {
    let registry = ConcurrencyLimiterRegistry::new(10);
    registry.register("critical_fn", 2);

    let limiter = registry.get_or_create("critical_fn");
    assert_eq!(limiter.max_concurrent(), 2);
}
