#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

#[test]
fn off_is_one_attempt_and_the_default() {
    assert_eq!(TransactionRetry::off().max_attempts(), 1);
    assert_eq!(TransactionRetry::default().max_attempts(), 1);
}

#[test]
fn each_bound_is_refused_by_name() {
    for (max_attempts, base_ms, max_ms, named) in [
        (0, 10, 100, "max_attempts"),
        (3, 200, 100, "base_ms"),
        (3, 10, MAX_BACKOFF_MS + 1, "max_ms"),
    ] {
        let err = TransactionRetry::new(max_attempts, base_ms, max_ms)
            .expect_err("the bound must be refused");
        assert!(
            matches!(&err, FraiseQLError::Configuration { message } if message.contains(named)),
            "{named}: {err:?}"
        );
    }
    TransactionRetry::new(1, 0, 0).unwrap();
    TransactionRetry::new(10, MAX_BACKOFF_MS, MAX_BACKOFF_MS).unwrap();
}

/// Ruling (b): whatever the jitter draws, one wait never exceeds `max_ms`, however
/// many retries came before (no overflow past the cap either).
#[test]
fn no_wait_exceeds_max_ms() {
    let policy = TransactionRetry::new(u32::MAX, 7, 100).unwrap();
    for retry in [1, 2, 3, 4, 5, 6, 7, 8, 31, 32, 33, 63, 64, 65, u32::MAX] {
        for entropy in [
            0,
            1,
            99,
            100,
            101,
            107,
            u64::MAX / 2,
            u64::MAX - 1,
            u64::MAX,
        ] {
            let wait = policy.backoff(retry, entropy);
            assert!(
                wait <= Duration::from_millis(100),
                "retry {retry}, entropy {entropy}: {wait:?}"
            );
        }
    }
}

/// The `n`th retry waits at most `base · 2ⁿ⁻¹`, so an early retry stays short.
#[test]
fn a_wait_grows_from_base() {
    let policy = TransactionRetry::new(10, 10, 5_000).unwrap();
    for entropy in [0, 1, 9, 10, 11, 19, 20, 21, 39, 40, 41, u64::MAX] {
        assert!(policy.backoff(1, entropy) <= Duration::from_millis(10), "{entropy}");
        assert!(policy.backoff(2, entropy) <= Duration::from_millis(20), "{entropy}");
        assert!(policy.backoff(3, entropy) <= Duration::from_millis(40), "{entropy}");
    }
    assert_eq!(policy.backoff(3, 40), Duration::from_millis(40));
}
