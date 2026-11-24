//! An entry's expiry saturates at the last instant a `Timestamp` holds.

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use fraiseql_time::{Timestamp, jiff::SignedDuration};

use super::expiry;

#[test]
fn an_expiry_within_the_calendar_is_now_plus_the_ttl() {
    let now = Timestamp::from_second(1_763_640_000).unwrap();
    assert_eq!(
        expiry(now, SignedDuration::from_secs(3600)),
        Timestamp::from_second(1_763_643_600).unwrap()
    );
}

#[test]
fn an_expiry_past_the_calendar_is_its_last_instant() {
    let now = Timestamp::from_second(1_763_640_000).unwrap();
    let ttl = SignedDuration::from_secs(i64::MAX);
    assert_eq!(expiry(now, ttl).to_rfc3339(), "9999-12-30T22:00:00.999999999+00:00");
}
