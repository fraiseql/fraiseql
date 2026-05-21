#![allow(clippy::unwrap_used)] // Reason: test/bench code, panics are acceptable
//! Rate limiter contention stress tests.
//!
//! Verifies exact limit enforcement under high thread contention. Uses
//! `std::thread::spawn` (not `tokio::spawn`) because `KeyedRateLimiter::check`
//! is synchronous and holds a `Mutex` for the entire operation.

use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use fraiseql_auth::{AuthRateLimitConfig, KeyedRateLimiter};

#[test]
fn test_100_threads_exact_limit_enforcement() {
    let limiter = Arc::new(KeyedRateLimiter::new(AuthRateLimitConfig {
        enabled:      true,
        max_requests: 50,
        window_secs:  3600,
    }));

    let allowed = Arc::new(AtomicU32::new(0));
    let rejected = Arc::new(AtomicU32::new(0));

    let handles: Vec<_> = (0..100)
        .map(|_| {
            let limiter = Arc::clone(&limiter);
            let allowed = Arc::clone(&allowed);
            let rejected = Arc::clone(&rejected);
            std::thread::spawn(move || {
                match limiter.check("shared_key") {
                    Ok(()) => allowed.fetch_add(1, Ordering::SeqCst),
                    Err(_) => rejected.fetch_add(1, Ordering::SeqCst),
                };
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let a = allowed.load(Ordering::SeqCst);
    let r = rejected.load(Ordering::SeqCst);
    assert_eq!(a, 50, "exactly max_requests should be allowed");
    assert_eq!(r, 50, "remaining should be rejected");
}

#[test]
fn test_per_key_isolation_under_contention() {
    let limiter = Arc::new(KeyedRateLimiter::new(AuthRateLimitConfig {
        enabled:      true,
        max_requests: 10,
        window_secs:  3600,
    }));

    let allowed = Arc::new(AtomicU32::new(0));

    // 10 keys x 10 threads, each thread checks its own key once
    let handles: Vec<_> = (0..100)
        .map(|i| {
            let limiter = Arc::clone(&limiter);
            let allowed = Arc::clone(&allowed);
            let key = format!("key_{}", i / 10);
            std::thread::spawn(move || {
                if limiter.check(&key).is_ok() {
                    allowed.fetch_add(1, Ordering::SeqCst);
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    assert_eq!(
        allowed.load(Ordering::SeqCst),
        100,
        "all 100 requests should be allowed (10 per key, 10 keys)"
    );
}

#[test]
fn test_concurrent_check_and_clear() {
    let limiter = Arc::new(KeyedRateLimiter::new(AuthRateLimitConfig {
        enabled:      true,
        max_requests: 50,
        window_secs:  3600,
    }));

    let mut handles = Vec::with_capacity(101);

    // 100 checker threads
    for _ in 0..100 {
        let limiter = Arc::clone(&limiter);
        handles.push(std::thread::spawn(move || {
            let _ = limiter.check("contended_key");
        }));
    }

    // 1 clearer thread interleaved with checkers
    {
        let limiter = Arc::clone(&limiter);
        handles.push(std::thread::spawn(move || {
            limiter.clear();
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // No panics occurred — that's the assertion
}

#[test]
fn test_high_contention_single_key() {
    let limiter = Arc::new(KeyedRateLimiter::new(AuthRateLimitConfig {
        enabled:      true,
        max_requests: 100,
        window_secs:  3600,
    }));

    let allowed = Arc::new(AtomicU32::new(0));
    let rejected = Arc::new(AtomicU32::new(0));

    let handles: Vec<_> = (0..1000)
        .map(|_| {
            let limiter = Arc::clone(&limiter);
            let allowed = Arc::clone(&allowed);
            let rejected = Arc::clone(&rejected);
            std::thread::spawn(move || {
                match limiter.check("hot_key") {
                    Ok(()) => allowed.fetch_add(1, Ordering::SeqCst),
                    Err(_) => rejected.fetch_add(1, Ordering::SeqCst),
                };
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let a = allowed.load(Ordering::SeqCst);
    let r = rejected.load(Ordering::SeqCst);
    assert_eq!(a, 100, "exactly max_requests should be allowed");
    assert_eq!(r, 900, "remaining should be rejected");
    assert_eq!(a + r, 1000, "all requests should be accounted for");
}
