use super::*;

#[test]
fn test_constant_time_eq_equal() {
    assert!(constant_time_eq(b"test", b"test"));
    assert!(constant_time_eq(b"", b""));
}

#[test]
fn test_constant_time_eq_not_equal() {
    assert!(!constant_time_eq(b"test", b"fail"));
    assert!(!constant_time_eq(b"test", b"tes"));
    assert!(!constant_time_eq(b"test", b""));
}

// ── M-webhook-replay-drift: shared timestamp-freshness check ──────────────────

#[test]
fn freshness_accepts_timestamp_inside_window() {
    // 100s old, 300s tolerance → fresh.
    assert!(check_timestamp_freshness(1_000_100, "1000000", 300).is_ok());
}

#[test]
fn freshness_rejects_stale_timestamp() {
    // 1000s old, 300s tolerance → stale.
    assert!(matches!(
        check_timestamp_freshness(1_001_000, "1000000", 300),
        Err(SignatureError::TimestampExpired)
    ));
}

#[test]
fn freshness_rejects_future_timestamp_beyond_window() {
    // 1000s in the future, 300s tolerance → rejected.
    assert!(matches!(
        check_timestamp_freshness(1_000_000, "1001000", 300),
        Err(SignatureError::TimestampExpired)
    ));
}

#[test]
fn freshness_rejects_non_numeric_timestamp() {
    assert!(matches!(
        check_timestamp_freshness(1_000_000, "not-a-number", 300),
        Err(SignatureError::InvalidFormat)
    ));
}

#[test]
fn freshness_huge_tolerance_does_not_wrap_to_reject_everything() {
    // A `u64` tolerance larger than `i64::MAX` must saturate, NOT wrap negative.
    // The old `seconds as i64` cast wrapped, yielding a negative window that
    // rejected every request (M-webhook-replay-drift). A fresh request must
    // still verify under an effectively-infinite tolerance.
    assert!(check_timestamp_freshness(1_000_000, "1000000", u64::MAX).is_ok());
    // And even a wildly out-of-window timestamp is accepted (window is infinite).
    assert!(check_timestamp_freshness(i64::MAX, "0", u64::MAX).is_ok());
}
