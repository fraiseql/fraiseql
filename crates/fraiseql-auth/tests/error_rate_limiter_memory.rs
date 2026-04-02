#![allow(clippy::unwrap_used)] // Reason: test/bench code, panics are acceptable
//! Rate limiter memory and boundary tests.
//!
//! Tests `KeyedRateLimiter` behavior under high key cardinality and verifies
//! that `clear()` reclaims tracked entries.

use fraiseql_auth::{AuthRateLimitConfig, KeyedRateLimiter};

const fn high_limit_config() -> AuthRateLimitConfig {
    AuthRateLimitConfig {
        enabled: true,
        max_requests: 1_000_000,
        window_secs: 3600,
    }
}

#[test]
fn test_rate_limiter_handles_many_unique_keys() {
    let limiter = KeyedRateLimiter::new(high_limit_config());

    for i in 0..100_000 {
        let key = format!("ip-{i}");
        limiter.check(&key).unwrap();
    }

    assert_eq!(limiter.active_limiters(), 100_000);
}

#[test]
fn test_rate_limiter_memory_bounded_by_entry_count() {
    let limiter = KeyedRateLimiter::new(high_limit_config());

    let n = 10_000;
    for i in 0..n {
        limiter.check(&format!("user-{i}")).unwrap();
    }

    assert_eq!(limiter.active_limiters(), n);
}

#[test]
fn test_rate_limiter_cap_evicts_oldest_when_full() {
    // Build a limiter with a tiny cap to verify LRU eviction behavior.
    let limiter = KeyedRateLimiter::with_max_entries(high_limit_config(), 3);

    limiter.check("ip-1").unwrap();
    limiter.check("ip-2").unwrap();
    limiter.check("ip-3").unwrap();
    assert_eq!(limiter.active_limiters(), 3);

    // 4th key succeeds via LRU eviction
    limiter.check("ip-4").expect("should succeed via eviction");
    assert_eq!(limiter.active_limiters(), 3); // still at capacity

    // ip-1 was evicted (oldest), so re-inserting it succeeds
    limiter.check("ip-1").expect("evicted key can re-enter");
}

#[test]
fn test_rate_limiter_cap_zero_disables_limit() {
    // cap = 0 means unbounded
    let limiter = KeyedRateLimiter::with_max_entries(high_limit_config(), 0);

    for i in 0..10_000 {
        limiter.check(&format!("ip-{i}")).unwrap();
    }
    assert_eq!(limiter.active_limiters(), 10_000);
}

#[test]
fn test_rate_limiter_disabled_skips_tracking() {
    let config = AuthRateLimitConfig {
        enabled: false,
        max_requests: 10,
        window_secs: 60,
    };
    let limiter = KeyedRateLimiter::new(config);

    for i in 0..10_000 {
        limiter.check(&format!("key-{i}")).unwrap();
    }

    assert_eq!(limiter.active_limiters(), 0);
}

#[test]
fn test_rate_limiter_clear_reclaims_memory() {
    let limiter = KeyedRateLimiter::new(high_limit_config());

    for i in 0..5_000 {
        limiter.check(&format!("ip-{i}")).unwrap();
    }
    assert_eq!(limiter.active_limiters(), 5_000);

    limiter.clear();
    assert_eq!(limiter.active_limiters(), 0);
}
