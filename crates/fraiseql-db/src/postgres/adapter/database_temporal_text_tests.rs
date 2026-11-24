//! The response text of a `timestamptz`, `timestamp` and `date` cell, byte for byte.
//!
//! `decode_value` writes it into the response JSON, so a client reads exactly these
//! strings. Each cell is built in PostgreSQL's binary form (microseconds, or days, since
//! 2000-01-01), so every case is decoded as the driver hands it over, with no server.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, a failed decode is a test failure

use tokio_postgres::types::Type;

use super::decode_value;

/// Microseconds since 2000-01-01 00:00:00 UTC, as PostgreSQL stores a timestamp.
fn micros(micros: i64) -> [u8; 8] {
    micros.to_be_bytes()
}

/// Days since 2000-01-01, as PostgreSQL stores a date.
fn days(days: i32) -> [u8; 4] {
    days.to_be_bytes()
}

fn text(ty: &Type, raw: &[u8]) -> String {
    match decode_value(ty, raw).unwrap() {
        serde_json::Value::String(text) => text,
        other => panic!("a {} cell decoded to {other:?}, not a string", ty.name()),
    }
}

// 2024-01-15T10:30:00 UTC, and that instant plus a fraction.
const AT: i64 = 758_629_800_000_000;

#[test]
fn a_timestamptz_is_rfc_3339_with_a_utc_offset_and_the_fraction_in_3_6_or_9_digits() {
    let cases = [
        (AT, "2024-01-15T10:30:00+00:00"),
        (AT + 500_000, "2024-01-15T10:30:00.500+00:00"),
        (AT + 123_400, "2024-01-15T10:30:00.123400+00:00"),
        (AT + 123_456, "2024-01-15T10:30:00.123456+00:00"),
        (AT + 1, "2024-01-15T10:30:00.000001+00:00"),
        // The last microsecond before 1970: a negative offset from the Unix epoch.
        (-946_684_800_000_001, "1969-12-31T23:59:59.999999+00:00"),
    ];
    for (at, want) in cases {
        assert_eq!(text(&Type::TIMESTAMPTZ, &micros(at)), want, "timestamptz {at}");
    }
}

#[test]
fn a_timestamptz_outside_four_digit_years_keeps_its_signed_year() {
    // 45 BC (astronomical year -44), and the year 10000: both inside PostgreSQL's range.
    let cases = [
        (-64_496_001_600_000_000, "-0044-03-15T12:00:00+00:00"),
        (252_455_616_000_000_000, "+10000-01-01T00:00:00+00:00"),
    ];
    for (at, want) in cases {
        assert_eq!(text(&Type::TIMESTAMPTZ, &micros(at)), want, "timestamptz {at}");
    }
}

#[test]
fn a_timestamp_is_a_local_date_time_with_no_offset_and_the_fraction_in_3_6_or_9_digits() {
    let cases = [
        (AT, "2024-01-15T10:30:00"),
        (AT + 500_000, "2024-01-15T10:30:00.500"),
        (AT + 123_400, "2024-01-15T10:30:00.123400"),
        (AT + 123_456, "2024-01-15T10:30:00.123456"),
        (AT + 1, "2024-01-15T10:30:00.000001"),
        (-946_684_800_000_001, "1969-12-31T23:59:59.999999"),
        (-64_496_001_600_000_000, "-0044-03-15T12:00:00"),
        (252_455_616_000_000_000, "+10000-01-01T00:00:00"),
    ];
    for (at, want) in cases {
        assert_eq!(text(&Type::TIMESTAMP, &micros(at)), want, "timestamp {at}");
    }
}

#[test]
fn a_date_is_an_iso_8601_date_with_a_signed_year_outside_four_digits() {
    let cases = [
        (8_780, "2024-01-15"),
        (-730_119, "0001-01-01"),
        (-746_482, "-0044-03-15"),
        (2_921_940, "+10000-01-01"),
    ];
    for (at, want) in cases {
        assert_eq!(text(&Type::DATE, &days(at)), want, "date {at}");
    }
}

#[test]
fn an_infinite_timestamp_or_date_is_refused_not_rendered() {
    // PostgreSQL's `infinity` and `-infinity` are the extreme stored values.
    for ty in [Type::TIMESTAMPTZ, Type::TIMESTAMP] {
        for raw in [i64::MAX, i64::MIN] {
            assert!(decode_value(&ty, &micros(raw)).is_err(), "{} {raw}", ty.name());
        }
    }
    for raw in [i32::MAX, i32::MIN] {
        assert!(decode_value(&Type::DATE, &days(raw)).is_err(), "date {raw}");
    }
}
