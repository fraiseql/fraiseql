#![allow(clippy::unwrap_used)] // Reason: test/bench code, panics are acceptable
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

// =============================================================================
// F057 acceptance: strict capacity cap on the insert path
// =============================================================================

/// F057 unit test: inserting `max_entries + 1` keys sequentially must evict
/// exactly one (the oldest) and leave `len == max_entries`.
#[test]
fn test_rate_limiter_strict_cap_on_sequential_overflow() {
    let cap = 5;
    let limiter = KeyedRateLimiter::with_max_entries(high_limit_config(), cap);

    for i in 0..cap {
        limiter.check(&format!("key-{i}")).unwrap();
    }
    assert_eq!(limiter.active_limiters(), cap, "limiter must fill to cap");

    // The +1 insert must evict the oldest (key-0) and keep len == cap.
    limiter.check("key-overflow").unwrap();
    assert_eq!(
        limiter.active_limiters(),
        cap,
        "len must never exceed max_entries on the insert path"
    );

    // Re-inserting key-0 (the evictee) must succeed — it counts as a new key.
    limiter.check("key-0").unwrap();
    assert_eq!(limiter.active_limiters(), cap, "still at cap after re-insert");
}

/// F057 integration test: drive `max_entries + 100` concurrent inserts from
/// many threads and sample `active_limiters()` mid-flight.  The strict-cap
/// design guarantees `len <= max_entries` at **every** observable instant,
/// not just after the burst settles.
///
/// A best-effort cap (the pre-fix behaviour) would intermittently observe
/// `len > max_entries` during the burst as concurrent inserters raced past
/// the cap-check on different shards.
#[test]
fn test_rate_limiter_strict_cap_under_concurrent_burst() {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
        thread,
    };

    let cap = 32;
    let limiter = Arc::new(KeyedRateLimiter::with_max_entries(high_limit_config(), cap));

    let stop = Arc::new(AtomicBool::new(false));
    let observed_max = Arc::new(AtomicUsize::new(0));

    // Sampler thread: continuously reads len() and records the high-water mark.
    let sampler_limiter = Arc::clone(&limiter);
    let sampler_stop = Arc::clone(&stop);
    let sampler_max = Arc::clone(&observed_max);
    let sampler = thread::spawn(move || {
        while !sampler_stop.load(Ordering::Relaxed) {
            let len = sampler_limiter.active_limiters();
            // Update high-water mark with a CAS loop (lock-free max).
            let mut cur = sampler_max.load(Ordering::Relaxed);
            while len > cur {
                match sampler_max.compare_exchange_weak(
                    cur,
                    len,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(actual) => cur = actual,
                }
            }
        }
    });

    // Writer threads: each contributes (cap + 100) / N inserts of distinct keys.
    let threads = 8;
    let inserts_per_thread = (cap + 100).div_ceil(threads);
    let mut writers = Vec::new();
    for t in 0..threads {
        let writer_limiter = Arc::clone(&limiter);
        writers.push(thread::spawn(move || {
            for i in 0..inserts_per_thread {
                let key = format!("t{t}-k{i}");
                writer_limiter.check(&key).unwrap();
            }
        }));
    }

    for w in writers {
        w.join().unwrap();
    }
    stop.store(true, Ordering::Relaxed);
    sampler.join().unwrap();

    // Post-burst assertion: after settling, len is exactly cap.
    assert_eq!(
        limiter.active_limiters(),
        cap,
        "post-burst len must equal cap (eviction kicked in for every overflow insert)"
    );

    // Strict-cap assertion: the high-water mark observed during the burst
    // never exceeded cap.  A best-effort cap would fail this assertion.
    let high_water = observed_max.load(Ordering::Relaxed);
    assert!(
        high_water <= cap,
        "active_limiters() observed {high_water} mid-burst but cap is {cap} — \
         strict-cap invariant violated"
    );
}
