//! The text chrono wrote, case by case (from chrono 0.4's own output), and the
//! saturation at the last instant.

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use fraiseql_time::Timestamp;

use super::{later_by, rfc3339_bound, rfc3339_millis_z, year_month};

fn at(second: i64, nanosecond: i32) -> Timestamp {
    Timestamp::new(second, nanosecond).unwrap()
}

#[test]
fn milliseconds_are_always_three_digits_truncated_with_z() {
    assert_eq!(rfc3339_millis_z(at(1_705_314_600, 0)), "2024-01-15T10:30:00.000Z");
    assert_eq!(rfc3339_millis_z(at(1_705_314_600, 123_999_999)), "2024-01-15T10:30:00.123Z");
    assert_eq!(rfc3339_millis_z(at(-1, 999_000_000)), "1969-12-31T23:59:59.999Z");
    assert_eq!(rfc3339_millis_z(at(-1, 0)), "1969-12-31T23:59:59.000Z");
}

#[test]
fn the_period_is_the_year_and_month() {
    assert_eq!(year_month(at(1_705_314_600, 0)), "2024-01");
    assert_eq!(year_month(at(-62_135_596_800, 0)), "0001-01");
    assert_eq!(year_month(at(253_402_207_200, 0)), "9999-12");
}

#[test]
fn a_length_past_the_last_instant_keeps_until_it() {
    let now = at(1_705_314_600, 0);
    assert_eq!(later_by(now, 3_600), at(1_705_318_200, 0));
    for secs in [300_000_000_000, i64::MAX.unsigned_abs(), u64::MAX] {
        assert_eq!(
            later_by(now, secs).to_rfc3339(),
            "9999-12-30T22:00:00.999999999+00:00",
            "{secs}"
        );
    }
}

#[test]
fn a_bound_is_read_as_chrono_read_it_and_past_the_last_instant_is_the_last_instant() {
    let read = |text| rfc3339_bound(text).map(Timestamp::to_rfc3339);
    assert_eq!(read("2024-01-15T12:30:00+02:00").as_deref(), Some("2024-01-15T10:30:00+00:00"));
    assert_eq!(read("2024-01-15T10:30:60Z").as_deref(), Some("2024-01-15T10:31:00+00:00"));
    for text in ["9999-12-31T23:59:59Z", "9999-12-31T23:59:59-23:59"] {
        assert_eq!(read(text).as_deref(), Some("9999-12-30T22:00:00.999999999+00:00"), "{text}");
    }
    for text in [
        "yesterday",
        "2024-01-15",
        "2024-01-15T10:30:00",
        "+10000-01-01T00:00:00Z",
    ] {
        assert_eq!(read(text), None, "{text}");
    }
}
