#![allow(clippy::unwrap_used)]  // Reason: test/bench code, panics are acceptable
//! Rate limiter memory and boundary tests.
//!
//! Tests `KeyedRateLimiter` behavior under high key cardinality and verifies
//! that `clear()` reclaims tracked entries.

use fraiseql_auth::{AuthRateLimitConfig, KeyedRateLimiter};

const fn high_limit_config() -> AuthRateLimitConfig {
    AuthRateLimitConfig {
        enabled:      true,
        max_requests: 1_000_000,
        window_secs:  3600,
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
    // Documents current behavior: no eviction, so entry count equals unique keys.
    // If eviction is added in the future, this test should be updated.
    let limiter = KeyedRateLimiter::new(high_limit_config());

    let n = 10_000;
    for i in 0..n {
        limiter.check(&format!("user-{i}")).unwrap();
    }

    // Without eviction, all entries are retained
    assert_eq!(limiter.active_limiters(), n);
}

#[test]
fn test_rate_limiter_disabled_skips_tracking() {
    let config = AuthRateLimitConfig {
        enabled:      false,
        max_requests: 10,
        window_secs:  60,
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
