//! The SQL literal an uploaded Arrow timestamp or `Date32` cell becomes, byte for byte, and
//! the client-supplied values that are refused rather than written.

#![allow(clippy::unwrap_used)] // Reason: test code, a failed conversion is a test failure

use std::sync::Arc;

use arrow::array::{
    Array, Date32Array, TimestampMicrosecondArray, TimestampMillisecondArray,
    TimestampNanosecondArray, TimestampSecondArray,
};

use super::arrow_value_to_sql;

// 2024-01-15T10:30:00Z, in seconds since the Unix epoch.
const AT: i64 = 1_705_314_600;

fn literal(array: Arc<dyn Array>) -> Result<String, String> {
    arrow_value_to_sql(&array, 0)
}

#[test]
fn a_timestamp_is_an_rfc_3339_timestamptz_literal_with_the_fraction_in_3_6_or_9_digits() {
    let cases: [(Arc<dyn Array>, &str); 7] = [
        (
            Arc::new(TimestampSecondArray::from(vec![AT])),
            "'2024-01-15T10:30:00+00:00'::timestamptz",
        ),
        (
            Arc::new(TimestampMillisecondArray::from(vec![AT * 1_000 + 500])),
            "'2024-01-15T10:30:00.500+00:00'::timestamptz",
        ),
        (
            Arc::new(TimestampMicrosecondArray::from(vec![AT * 1_000_000 + 123_400])),
            "'2024-01-15T10:30:00.123400+00:00'::timestamptz",
        ),
        (
            Arc::new(TimestampNanosecondArray::from(vec![AT * 1_000_000_000 + 123_456_789])),
            "'2024-01-15T10:30:00.123456789+00:00'::timestamptz",
        ),
        (
            Arc::new(TimestampMicrosecondArray::from(vec![-1])),
            "'1969-12-31T23:59:59.999999+00:00'::timestamptz",
        ),
        // 45 BC (astronomical year -44) and the year 10000.
        (
            Arc::new(TimestampSecondArray::from(vec![-63_549_316_800])),
            "'-0044-03-15T12:00:00+00:00'::timestamptz",
        ),
        (
            Arc::new(TimestampSecondArray::from(vec![253_402_300_800])),
            "'+10000-01-01T00:00:00+00:00'::timestamptz",
        ),
    ];
    for (array, want) in cases {
        assert_eq!(literal(array).unwrap(), want);
    }
}

#[test]
fn a_timestamp_past_the_representable_range_is_refused() {
    for seconds in [i64::MAX, i64::MIN] {
        assert!(
            literal(Arc::new(TimestampSecondArray::from(vec![seconds]))).is_err(),
            "{seconds}"
        );
    }
}

#[test]
fn a_date32_is_an_iso_8601_date_literal_with_a_signed_year_outside_four_digits() {
    let cases = [
        (19_737, "'2024-01-15'::date"),
        (-1, "'1969-12-31'::date"),
        (-735_525, "'-0044-03-15'::date"),
        (2_932_897, "'+10000-01-01'::date"),
    ];
    for (days, want) in cases {
        assert_eq!(literal(Arc::new(Date32Array::from(vec![days]))).unwrap(), want, "{days}");
    }
}

#[test]
fn a_date32_past_the_representable_range_is_refused() {
    for days in [i32::MAX, i32::MIN] {
        assert!(literal(Arc::new(Date32Array::from(vec![days]))).is_err(), "{days}");
    }
}
