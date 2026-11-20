//! Tests for the forms an instant is written and read in.

use jiff::SignedDuration;

use crate::{Error, Timestamp};

/// `(second, nanosecond, serde and Debug, to_rfc3339, Display)`, as chrono 0.4.45 wrote
/// each instant. The table is the pin; it does not depend on chrono.
const GOLDEN: &[(i64, i32, &str, &str, &str)] = &[
    (
        1_763_640_000,
        0,
        "2025-11-20T12:00:00Z",
        "2025-11-20T12:00:00+00:00",
        "2025-11-20 12:00:00 UTC",
    ),
    (
        1_763_640_000,
        120_000_000,
        "2025-11-20T12:00:00.120Z",
        "2025-11-20T12:00:00.120+00:00",
        "2025-11-20 12:00:00.120 UTC",
    ),
    (
        1_763_640_000,
        500_000_000,
        "2025-11-20T12:00:00.500Z",
        "2025-11-20T12:00:00.500+00:00",
        "2025-11-20 12:00:00.500 UTC",
    ),
    (
        1_763_640_000,
        1000,
        "2025-11-20T12:00:00.000001Z",
        "2025-11-20T12:00:00.000001+00:00",
        "2025-11-20 12:00:00.000001 UTC",
    ),
    (
        1_763_640_000,
        123_456_000,
        "2025-11-20T12:00:00.123456Z",
        "2025-11-20T12:00:00.123456+00:00",
        "2025-11-20 12:00:00.123456 UTC",
    ),
    (
        1_763_640_000,
        123_456_789,
        "2025-11-20T12:00:00.123456789Z",
        "2025-11-20T12:00:00.123456789+00:00",
        "2025-11-20 12:00:00.123456789 UTC",
    ),
    (
        1_763_640_000,
        1,
        "2025-11-20T12:00:00.000000001Z",
        "2025-11-20T12:00:00.000000001+00:00",
        "2025-11-20 12:00:00.000000001 UTC",
    ),
    (
        0,
        0,
        "1970-01-01T00:00:00Z",
        "1970-01-01T00:00:00+00:00",
        "1970-01-01 00:00:00 UTC",
    ),
    (
        -1,
        500_000_000,
        "1969-12-31T23:59:59.500Z",
        "1969-12-31T23:59:59.500+00:00",
        "1969-12-31 23:59:59.500 UTC",
    ),
    (
        946_684_800,
        0,
        "2000-01-01T00:00:00Z",
        "2000-01-01T00:00:00+00:00",
        "2000-01-01 00:00:00 UTC",
    ),
    (
        -62_135_596_800,
        0,
        "0001-01-01T00:00:00Z",
        "0001-01-01T00:00:00+00:00",
        "0001-01-01 00:00:00 UTC",
    ),
    (
        -62_167_219_200,
        0,
        "0000-01-01T00:00:00Z",
        "0000-01-01T00:00:00+00:00",
        "0000-01-01 00:00:00 UTC",
    ),
    (
        -62_198_755_200,
        0,
        "-0001-01-01T00:00:00Z",
        "-0001-01-01T00:00:00+00:00",
        "-0001-01-01 00:00:00 UTC",
    ),
    (
        -210_866_803_200,
        0,
        "-4713-11-24T00:00:00Z",
        "-4713-11-24T00:00:00+00:00",
        "-4713-11-24 00:00:00 UTC",
    ),
    (
        -377_705_023_201,
        0,
        "-9999-01-02T01:59:59Z",
        "-9999-01-02T01:59:59+00:00",
        "-9999-01-02 01:59:59 UTC",
    ),
    (
        253_402_207_200,
        999_999_999,
        "9999-12-30T22:00:00.999999999Z",
        "9999-12-30T22:00:00.999999999+00:00",
        "9999-12-30 22:00:00.999999999 UTC",
    ),
];

/// Text and the instant chrono 0.4.45 read from it, in microseconds from the Unix
/// epoch, or `None` where it refused the text.
const PARSED: &[(&str, Option<i64>)] = &[
    ("2025-11-20T12:00:00Z", Some(1_763_640_000_000_000)),
    ("2025-11-20T12:00:00.12Z", Some(1_763_640_000_120_000)),
    ("2025-11-20T12:00:00.120+00:00", Some(1_763_640_000_120_000)),
    ("2025-11-20T12:00:00+02:00", Some(1_763_632_800_000_000)),
    ("2025-11-20T12:00:00-05:30", Some(1_763_659_800_000_000)),
    ("2025-11-20T12:00:00−05:30", Some(1_763_659_800_000_000)),
    ("2025-11-20 12:00:00Z", Some(1_763_640_000_000_000)),
    ("2025-11-20t12:00:00z", Some(1_763_640_000_000_000)),
    ("2025-11-20T12:00:00.123456789Z", Some(1_763_640_000_123_456)),
    ("2025-11-20T12:00:00.1234567891Z", Some(1_763_640_000_123_456)),
    ("2025-11-20T12:00:00 UTC", Some(1_763_640_000_000_000)),
    ("2025-11-20T12:00:00utc", Some(1_763_640_000_000_000)),
    ("2025-11-20T12:00:00 +00:00", Some(1_763_640_000_000_000)),
    ("2025-11-20T12:00:00+0530", Some(1_763_620_200_000_000)),
    ("2025-11-20T12:00:00+05 30", Some(1_763_620_200_000_000)),
    ("2025-11-20T12:00:00+05::30", Some(1_763_620_200_000_000)),
    ("2025-11-20T12:00:00+23:59", Some(1_763_553_660_000_000)),
    (" 2025-11-20T12:00:00Z", Some(1_763_640_000_000_000)),
    ("2025-11-20T12:00:00Z ", Some(1_763_640_000_000_000)),
    ("2025 - 11 - 20T12 : 00 : 00Z", Some(1_763_640_000_000_000)),
    ("2025-1-2T3:4:5Z", Some(1_735_787_045_000_000)),
    ("25-11-20T12:00:00Z", Some(-61_350_264_000_000_000)),
    ("+2025-11-20T12:00:00Z", Some(1_763_640_000_000_000)),
    ("-0001-01-01T00:00:00Z", Some(-62_198_755_200_000_000)),
    ("0000-01-01T00:00:00Z", Some(-62_167_219_200_000_000)),
    ("-4713-11-24T00:00:00Z", Some(-210_866_803_200_000_000)),
    ("-9999-01-02T01:59:59Z", Some(-377_705_023_201_000_000)),
    ("9999-12-30T22:00:00.999999999Z", Some(253_402_207_200_999_999)),
    ("2025-11-20T12:00:60Z", Some(1_763_640_060_000_000)),
    ("2025-11-20T23:59:60.5Z", Some(1_763_683_200_500_000)),
    ("2024-02-29T12:00:00Z", Some(1_709_208_000_000_000)),
    ("2025-11-20T12:00:00", None),
    ("2025-11-20", None),
    ("2025-11-20T12:00Z", None),
    ("2025-13-20T12:00:00Z", None),
    ("2025-02-29T12:00:00Z", None),
    ("2025-11-20T24:00:00Z", None),
    ("2025-11-20T12:60:00Z", None),
    ("2025-11-20T12:00:61Z", None),
    ("2025-11-20T12:00:00+24:00", None),
    ("2025-11-20T12:00:00+05:60", None),
    ("2025-11-20T12:00:00+00", None),
    ("2025-11-20T12:00:00,5Z", None),
    ("2025-11-20T12:00:00.Z", None),
    ("2025-11-20T12:00:00Z[Europe/Paris]", None),
    ("2025-11-20T12:00:00ZZ", None),
    ("2025-11-20T12:00:00UTCX", None),
    ("20251120T120000Z", None),
    ("2025-11-20T120000Z", None),
    ("2025-11-20  12:00:00Z", Some(1_763_640_000_000_000)),
    ("2025-11-20T12:00:00+01:30:15", None),
    ("2025-W47-4T12:00:00Z", None),
    ("02025-11-20T12:00:00Z", None),
    ("not a time", None),
    ("", None),
];

#[test]
fn text_reads_as_chrono_read_it() {
    for &(text, micros) in PARSED {
        let read = Timestamp::parse_rfc3339(text).ok().map(Timestamp::as_microsecond);
        assert_eq!(read, micros, "{text:?}");
    }
}

/// chrono read years past ±9999; a `Timestamp` does not hold them.
#[test]
fn text_beyond_the_range_is_refused() {
    for text in [
        "-9999-01-01T00:00:00Z",
        "9999-12-31T23:59:59Z",
        "+10000-01-01T00:00:00Z",
    ] {
        let error = Timestamp::parse_rfc3339(text).expect_err("out of range");
        assert_eq!(
            error,
            Error::Parse {
                input: text.to_owned(),
            },
            "{text:?}"
        );
    }
}

fn at(second: i64, nanosecond: i32) -> Timestamp {
    Timestamp::new(second, nanosecond).expect("an instant in range")
}

#[test]
fn every_form_writes_the_golden_bytes() {
    for &(second, nanosecond, serde_form, rfc3339, display) in GOLDEN {
        let instant = at(second, nanosecond);
        assert_eq!(
            serde_json::to_string(&instant).expect("it serializes"),
            format!("\"{serde_form}\""),
            "serde, {second} s {nanosecond} ns"
        );
        assert_eq!(format!("{instant:?}"), serde_form, "Debug, {second} s {nanosecond} ns");
        assert_eq!(instant.to_rfc3339(), rfc3339, "to_rfc3339, {second} s {nanosecond} ns");
        assert_eq!(instant.to_string(), display, "Display, {second} s {nanosecond} ns");
    }
}

#[test]
fn every_golden_text_reads_back_as_its_instant() {
    for &(second, nanosecond, serde_form, rfc3339, _) in GOLDEN {
        let instant = at(second, nanosecond);
        let read: Timestamp =
            serde_json::from_str(&format!("\"{serde_form}\"")).expect("the serde form reads");
        assert_eq!(read, instant, "serde form {serde_form}");
        assert_eq!(
            Timestamp::parse_rfc3339(rfc3339).expect("the rfc3339 form reads"),
            instant,
            "{rfc3339}"
        );
    }
}

/// The same forms, compared with chrono itself over the golden instants and a grid of
/// fractions, while chrono is still a workspace dependency.
mod against_chrono {
    use chrono::{DateTime, Utc};

    use super::{GOLDEN, at};

    fn instants() -> Vec<(i64, i32)> {
        let mut all: Vec<(i64, i32)> = GOLDEN.iter().map(|&(s, n, ..)| (s, n)).collect();
        for second in [-1_000_000_001, -1, 0, 1_763_640_000] {
            for nanosecond in [
                0,
                1,
                10,
                999,
                1_000,
                10_000,
                999_000,
                1_000_000,
                10_000_000,
                100_000_000,
                120_000_000,
                123_000_000,
                123_400_000,
                123_450_000,
                123_456_700,
                999_999_999,
            ] {
                all.push((second, nanosecond));
            }
        }
        all
    }

    #[test]
    fn every_form_matches_chrono() {
        for (second, nanosecond) in instants() {
            let ours = at(second, nanosecond);
            let theirs: DateTime<Utc> =
                DateTime::from_timestamp(second, nanosecond.unsigned_abs()).expect("in range");
            let what = format!("{second} s {nanosecond} ns");
            assert_eq!(
                serde_json::to_string(&ours).expect("ours serializes"),
                serde_json::to_string(&theirs).expect("chrono serializes"),
                "serde, {what}"
            );
            assert_eq!(format!("{ours:?}"), format!("{theirs:?}"), "Debug, {what}");
            assert_eq!(ours.to_rfc3339(), theirs.to_rfc3339(), "to_rfc3339, {what}");
            assert_eq!(ours.to_string(), theirs.to_string(), "Display, {what}");
        }
    }

    /// Text chrono read, or refused, this type reads or refuses alike.
    #[test]
    fn parsing_matches_chrono() {
        for &(text, _) in super::PARSED {
            let ours =
                super::Timestamp::parse_rfc3339(text).ok().map(super::Timestamp::as_microsecond);
            let theirs = text
                .parse::<DateTime<chrono::FixedOffset>>()
                .ok()
                .map(|t| t.with_timezone(&Utc).timestamp_micros());
            assert_eq!(ours, theirs, "{text:?}");
        }
    }
}

#[test]
#[cfg(any(feature = "sqlx", feature = "postgres"))]
fn a_timestamp_is_written_in_whole_microseconds_from_2000_truncated_toward_it() {
    use crate::pg::to_pg_microseconds;

    for (second, nanosecond, microseconds) in [
        (946_684_800, 0, 0),
        (946_684_800, 1_999, 1),
        (946_684_799, 999_999_999, 0),
        (946_684_799, 999_999, -999_000),
        (1_763_640_000, 123_456_789, 816_955_200_123_456),
        (-1, 999_999_999, -946_684_800_000_000),
    ] {
        assert_eq!(
            to_pg_microseconds(at(second, nanosecond)),
            microseconds,
            "{second} s {nanosecond} ns"
        );
    }
}

#[test]
#[cfg(any(feature = "sqlx", feature = "postgres"))]
fn infinity_and_values_out_of_range_are_refused_by_name() {
    use crate::pg::from_pg_microseconds;

    for (microseconds, named) in [
        (i64::MAX, "timestamptz infinity is outside"),
        (i64::MIN, "timestamptz -infinity is outside"),
        (252_455_615_999_999_999, "timestamptz 252455615999999999 µs from"),
    ] {
        let error = from_pg_microseconds(microseconds).expect_err("refused");
        assert!(matches!(error, Error::OutOfRange { .. }), "{error:?}");
        assert!(error.to_string().contains(named), "{error}");
    }
}

#[test]
fn arithmetic_is_jiffs_and_checked_arithmetic_refuses_the_range() {
    let start = at(1_763_640_000, 0);
    assert_eq!(start + SignedDuration::from_mins(1), at(1_763_640_060, 0), "plus a minute");
    assert_eq!(start - at(1_763_639_940, 0), SignedDuration::from_mins(1), "a difference");
    assert_eq!(
        Timestamp::from_second(253_402_207_200)
            .expect("in range")
            .checked_add(SignedDuration::from_secs(1)),
        None,
        "past the top of the range"
    );
}
