//! The times a key version and the rotation metrics keep, stated as offsets from the
//! current time so the assertions do not depend on the date-time library behind them.
#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code, panics acceptable

use super::{KeyVersionMetadata, KeyVersionStatus, RotationMetrics};

type Instant = fraiseql_time::Timestamp;

fn now() -> Instant {
    fraiseql_time::Timestamp::now()
}

fn plus_seconds(at: Instant, seconds: i64) -> Instant {
    at + fraiseql_time::jiff::SignedDuration::from_secs(seconds)
}

fn seconds_between(from: Instant, to: Instant) -> i64 {
    (to - from).as_secs()
}

const DAY: i64 = 86_400;

/// A version issued `issued` seconds ago that expires `expires` seconds from now.
fn version(issued: i64, expires: i64) -> KeyVersionMetadata {
    let mut metadata = KeyVersionMetadata::new(1, 1);
    let at = now();
    metadata.issued_at = plus_seconds(at, -issued);
    metadata.expires_at = plus_seconds(at, expires);
    metadata
}

/// A version expires its TTL in whole days after it was issued.
#[test]
fn a_version_expires_its_ttl_after_it_is_issued() {
    for days in [0_u32, 1, 30, 365] {
        let metadata = KeyVersionMetadata::new(1, days);
        let ttl = seconds_between(metadata.issued_at, metadata.expires_at);
        assert_eq!(ttl, i64::from(days) * DAY, "{days}");
    }
}

/// A TTL reaching past the last representable day never expires within it.
#[test]
fn a_ttl_past_the_calendar_never_expires_within_it() {
    let last_day: Instant = "9999-12-30T00:00:00Z".parse().unwrap();
    for days in [4_000_000, u32::MAX] {
        let metadata = KeyVersionMetadata::new(1, days);
        assert!(metadata.expires_at >= last_day, "{days}: {:?}", metadata.expires_at);
        assert!(!metadata.is_expired(), "{days}");
        assert_eq!(metadata.ttl_consumed_percent(), 0, "{days}");
    }
}

/// The share of the TTL consumed is floored to a whole percent; a refresh is due from 80.
#[test]
fn the_consumed_share_is_a_floored_percent() {
    for (issued, expires, percent) in [
        (80 * DAY, 20 * DAY, 80),
        (79 * DAY, 21 * DAY, 79),
        (0, 100 * DAY, 0),
        (100 * DAY, -DAY, 100),
        (0, 0, 100),
        (0, -DAY, 100),
    ] {
        let metadata = version(issued, expires);
        assert_eq!(metadata.ttl_consumed_percent(), percent, "{issued} {expires}");
        assert_eq!(metadata.should_refresh(), percent >= 80, "{issued} {expires}");
    }
}

/// Fewer than 14 days left is expiring soon; past the expiry is expired, not expiring.
#[test]
fn a_version_is_expiring_in_its_last_fourteen_days() {
    for (expires, soon, expired, status) in [
        (15 * DAY, false, false, KeyVersionStatus::Active),
        (14 * DAY + 60, false, false, KeyVersionStatus::Active),
        (14 * DAY - 60, true, false, KeyVersionStatus::Expiring),
        (60, true, false, KeyVersionStatus::Expiring),
        (-60, false, true, KeyVersionStatus::Expired),
    ] {
        let mut metadata = version(DAY, expires);
        assert_eq!(metadata.is_expiring_soon(), soon, "{expires}");
        assert_eq!(metadata.is_expired(), expired, "{expires}");
        metadata.update_status();
        assert_eq!(metadata.status, status, "{expires}");
    }
}

/// The time left is the expiry less the current time, negative once it has passed.
#[test]
fn the_time_left_counts_down_to_the_expiry() {
    for expires in [20 * DAY, 60, -3600] {
        let left = version(DAY, expires).time_until_expiry();
        let left = left.as_secs();
        assert!((expires - 2..=expires).contains(&left), "{expires}: {left}");
    }
}

/// A rotation records when it happened.
#[test]
fn a_rotation_records_its_time() {
    let metrics = RotationMetrics::new();
    assert!(metrics.last_rotation().is_none());
    let before = now();
    metrics.record_rotation(5);
    let after = now();
    let at = metrics.last_rotation().expect("recorded");
    assert!(before <= at && at <= after, "{at:?}");
}
