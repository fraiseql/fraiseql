//! Which timestamp and date strings a row's JSON value converts to an Arrow
//! `Timestamp(Microsecond)` or `Date32`, and to which value: the accepted grammar, pinned
//! case by case, refusals included.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, a failed conversion is a test failure

use arrow::datatypes::{DataType, TimeUnit};
use serde_json::json;

use super::json_to_arrow_value;
use crate::convert::Value;

// 2024-01-15T10:30:00Z, in seconds since the Unix epoch.
const AT: i64 = 1_705_314_600;

fn timestamp(text: &str) -> Option<i64> {
    match json_to_arrow_value(&json!(text), &DataType::Timestamp(TimeUnit::Microsecond, None)) {
        Ok(Value::Timestamp(micros)) => Some(micros),
        Ok(other) => panic!("{text:?} converted to {other:?}, not a timestamp"),
        Err(_) => None,
    }
}

fn date(text: &str) -> Option<i32> {
    match json_to_arrow_value(&json!(text), &DataType::Date32) {
        Ok(Value::Date(days)) => Some(days),
        Ok(other) => panic!("{text:?} converted to {other:?}, not a date"),
        Err(_) => None,
    }
}

#[test]
fn an_rfc_3339_timestamp_converts_to_its_utc_instant_in_microseconds() {
    let cases = [
        ("2024-01-15T10:30:00Z", AT * 1_000_000),
        ("2024-01-15T10:30:00+00:00", AT * 1_000_000),
        ("2024-01-15T12:30:00+02:00", AT * 1_000_000),
        ("2024-01-15T10:30:00.5Z", AT * 1_000_000 + 500_000),
        ("2024-01-15T10:30:00.123456Z", AT * 1_000_000 + 123_456),
        // A fraction finer than a microsecond is truncated, not rounded.
        ("2024-01-15T10:30:00.123456789Z", AT * 1_000_000 + 123_456),
        ("1969-12-31T23:59:59.999999Z", -1),
        // The separators RFC 3339 allows besides `T` and `Z`.
        ("2024-01-15t10:30:00z", AT * 1_000_000),
        ("2024-01-15 10:30:00Z", AT * 1_000_000),
    ];
    for (text, want) in cases {
        assert_eq!(timestamp(text), Some(want), "{text:?}");
    }
}

#[test]
fn a_timestamp_with_no_offset_is_read_as_utc_to_the_whole_second() {
    assert_eq!(timestamp("2024-01-15T10:30:00"), Some(AT * 1_000_000));
}

#[test]
fn a_timestamp_outside_the_grammar_is_refused() {
    for text in [
        "2024-01-15T10:30:00.5",
        "2024-01-15",
        "2024-01-15T25:00:00Z",
        "2024-02-30T10:30:00Z",
        "2024-01-15T10:30Z",
        "",
    ] {
        assert_eq!(timestamp(text), None, "{text:?}");
    }
}

#[test]
fn a_date_converts_to_its_days_since_the_unix_epoch() {
    let cases = [
        ("2024-01-15", 19_737),
        ("1970-01-01", 0),
        ("1969-12-31", -1),
    ];
    for (text, want) in cases {
        assert_eq!(date(text), Some(want), "{text:?}");
    }
}

#[test]
fn a_date_outside_the_grammar_is_refused() {
    for text in [
        "2024-02-30",
        "2024-13-01",
        "2024-01-15T00:00:00",
        "15/01/2024",
        "",
    ] {
        assert_eq!(date(text), None, "{text:?}");
    }
}
