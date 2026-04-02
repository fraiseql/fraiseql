#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! Rate limiter time-based behavior tests.
//!
//! Tests window expiry (real 1-second sleep) and fail-open behavior
//! when the system clock is broken (injectable time source).
//!
//! **Execution engine:** none
//! **Infrastructure:** none
//! **Parallelism:** safe (each test creates an independent limiter)

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use fraiseql_auth::{AuthRateLimitConfig, KeyedRateLimiter};

// ── Window expiry ────────────────────────────────────────────────

#[test]
fn rate_limiter_resets_after_window_expires() {
    // Use a 1-second window so the test completes quickly.
    let config = AuthRateLimitConfig {
        enabled: true,
        max_requests: 3,
        window_secs: 1,
    };
    let limiter = KeyedRateLimiter::new(config);

    // Exhaust the window.
    for _ in 0..3 {
        limiter.check("key").unwrap();
    }
    assert!(limiter.check("key").is_err(), "must be blocked after exhausting the window");

    // Wait for the window to expire.
    std::thread::sleep(Duration::from_secs(2));

    // Must be allowed again in the fresh window.
    assert!(limiter.check("key").is_ok(), "must be allowed again after window expiry");
}

#[test]
fn rate_limiter_allows_exactly_max_requests_then_resets() {
    // Verify reset after window, not just the initial fill.
    let config = AuthRateLimitConfig {
        enabled: true,
        max_requests: 2,
        window_secs: 1,
    };
    let limiter = KeyedRateLimiter::new(config);

    // First window: fill and block.
    limiter.check("k").unwrap();
    limiter.check("k").unwrap();
    assert!(limiter.check("k").is_err(), "3rd request must be blocked");

    // After expiry: fill and block again — second window behaves identically.
    std::thread::sleep(Duration::from_secs(2));
    limiter.check("k").unwrap();
    limiter.check("k").unwrap();
    assert!(
        limiter.check("k").is_err(),
        "rate limiting must resume identically in second window"
    );
}

// ── System time fail-safe ────────────────────────────────────────

/// Simulated broken clock that always returns `u64::MAX`.
///
/// With `u64::MAX` as the current time, the window-expiry check:
/// `now >= record.window_start + window_secs`
/// evaluates as `u64::MAX >= (u64::MAX).wrapping_add(window_secs)` = `u64::MAX >= small_value`
/// which is always `true` due to unsigned overflow.
/// This means every request starts a new window and is allowed — fail-open behavior.
const fn broken_clock() -> u64 {
    u64::MAX
}

#[test]
fn rate_limiter_fails_open_on_broken_system_clock() {
    // A broken clock (returns u64::MAX) must cause all requests to be allowed.
    let limiter = KeyedRateLimiter::with_clock(
        AuthRateLimitConfig {
            enabled: true,
            max_requests: 3, // Normally blocks after 3 requests.
            window_secs: 60,
        },
        broken_clock,
    );

    // With a broken clock, far more than max_requests must be allowed.
    for i in 0..100 {
        assert!(
            limiter.check("key").is_ok(),
            "request {i}: broken clock must cause fail-open (all requests allowed)"
        );
    }
}

#[test]
fn rate_limiter_injectable_clock_controls_window_expiry() {
    // Demonstrate that the clock drives window-expiry logic correctly.
    // Start at t=0, advance to t=100 after filling the window.
    let time = Arc::new(AtomicU64::new(0));
    let time_for_limiter = Arc::clone(&time);

    let limiter = KeyedRateLimiter::with_clock(
        AuthRateLimitConfig {
            enabled: true,
            max_requests: 2,
            window_secs: 60,
        },
        move || time_for_limiter.load(Ordering::SeqCst),
    );

    // Fill the window at t=0.
    limiter.check("k").unwrap();
    limiter.check("k").unwrap();
    assert!(limiter.check("k").is_err(), "must block at t=0 after filling window");

    // Advance clock past window_secs.
    time.store(100, Ordering::SeqCst);

    // Must be allowed again.
    assert!(
        limiter.check("k").is_ok(),
        "must allow after advancing clock past window boundary"
    );
}

// ── Boundary condition ───────────────────────────────────────────

#[test]
fn rate_limiter_allows_exactly_max_requests() {
    // Verify the off-by-one comparison: `count < max_requests` (not `<=`).
    // With max_requests = 5, requests 1–5 must be allowed, request 6 must be denied.
    let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
        enabled: true,
        max_requests: 5,
        window_secs: 60,
    });

    for i in 0..5 {
        assert!(
            limiter.check("key").is_ok(),
            "request {i} (1-indexed: {}) must be allowed",
            i + 1
        );
    }
    assert!(limiter.check("key").is_err(), "request 6 must be denied (off-by-one check)");
}
