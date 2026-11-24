//! The calendar text, case by case, then over a sweep against chrono itself while chrono is
//! still a workspace dependency. PostgreSQL's own rendering is the other oracle, in
//! `fraiseql-db`'s `temporal_text_against_postgres_pg` suite.

use super::{date, date_time, rfc3339};

#[test]
fn a_date_is_written_with_chronos_year_rule() {
    let cases = [
        (0, "1970-01-01"),
        (-1, "1969-12-31"),
        (10_957, "2000-01-01"),
        (19_737, "2024-01-15"),
        (-719_162, "0001-01-01"),
        (-719_163, "0000-12-31"),
        (-735_525, "-0044-03-15"),
        (-2_440_550, "-4712-01-01"),
        (2_932_897, "+10000-01-01"),
    ];
    for (days, want) in cases {
        assert_eq!(date(days).as_deref(), Some(want), "{days}");
    }
}

#[test]
fn leap_days_follow_the_gregorian_rule_in_year_0_1900_2000_and_2400() {
    let cases = [
        (-719_469, "0000-02-29"),
        (-25_509, "1900-02-28"),
        (-25_508, "1900-03-01"),
        (11_016, "2000-02-29"),
        (157_113, "2400-02-29"),
    ];
    for (days, want) in cases {
        assert_eq!(date(days).as_deref(), Some(want), "{days}");
    }
}

#[test]
fn a_date_outside_the_years_chrono_held_is_none() {
    assert_eq!(date(-96_465_292).as_deref(), Some("-262143-01-01"));
    assert_eq!(date(95_026_236).as_deref(), Some("+262142-12-31"));
    // PostgreSQL's last timestamp day, 294276-12-31, and its last date, 5874897-12-31.
    for days in [
        -96_465_293,
        95_026_237,
        106_762_939,
        2_145_042_905,
        i64::MIN,
        i64::MAX,
    ] {
        assert_eq!(date(days), None, "{days}");
    }
}

#[test]
fn the_fraction_is_written_in_3_6_or_9_digits_and_left_out_at_a_whole_second() {
    let at = 1_705_314_600; // 2024-01-15T10:30:00Z
    let cases = [
        (0, "2024-01-15T10:30:00"),
        (500_000_000, "2024-01-15T10:30:00.500"),
        (123_400_000, "2024-01-15T10:30:00.123400"),
        (123_456_000, "2024-01-15T10:30:00.123456"),
        (1_000, "2024-01-15T10:30:00.000001"),
        (123_456_789, "2024-01-15T10:30:00.123456789"),
        (1, "2024-01-15T10:30:00.000000001"),
    ];
    for (nanosecond, want) in cases {
        assert_eq!(date_time(at, nanosecond).as_deref(), Some(want), "{nanosecond}");
    }
}

/// An instant before the epoch is a floored second and a fraction counted forward from
/// it: 1 µs before 1970 is the second -1 and 999 999 µs, so the text is
/// `23:59:59.999999` of the day before, not `00:00:00` truncated toward zero.
#[test]
fn an_instant_before_the_epoch_rounds_to_the_second_below_it() {
    assert_eq!(date_time(-1, 999_999_000).as_deref(), Some("1969-12-31T23:59:59.999999"));
    assert_eq!(date_time(-86_400, 0).as_deref(), Some("1969-12-31T00:00:00"));
    assert_eq!(date_time(-86_401, 500_000_000).as_deref(), Some("1969-12-30T23:59:59.500"));
}

#[test]
fn midnight_and_one_microsecond_either_side_of_it() {
    // 2000-01-01T00:00:00Z
    let midnight = 946_684_800;
    assert_eq!(
        date_time(midnight - 1, 999_999_000).as_deref(),
        Some("1999-12-31T23:59:59.999999")
    );
    assert_eq!(date_time(midnight, 0).as_deref(), Some("2000-01-01T00:00:00"));
    assert_eq!(date_time(midnight, 1_000).as_deref(), Some("2000-01-01T00:00:00.000001"));
}

#[test]
fn rfc3339_is_the_date_time_with_a_utc_offset_and_never_z() {
    assert_eq!(
        rfc3339(1_705_314_600, 500_000_000).as_deref(),
        Some("2024-01-15T10:30:00.500+00:00")
    );
    assert_eq!(rfc3339(253_402_300_800, 0).as_deref(), Some("+10000-01-01T00:00:00+00:00"));
}

#[test]
fn a_nanosecond_of_a_whole_second_or_a_date_outside_chronos_years_is_none() {
    assert_eq!(date_time(0, 1_000_000_000), None);
    assert_eq!(rfc3339(0, u32::MAX), None);
    for second in [
        i64::MIN,
        i64::MAX,
        95_026_237 * 86_400,
        -96_465_293 * 86_400,
    ] {
        assert_eq!(date_time(second, 0), None, "{second}");
    }
    // The last second chrono held, and the first.
    assert_eq!(date_time(95_026_237 * 86_400 - 1, 0).as_deref(), Some("+262142-12-31T23:59:59"));
    assert_eq!(date_time(-96_465_292 * 86_400, 0).as_deref(), Some("-262143-01-01T00:00:00"));
}

/// The same text as chrono's, over a sweep across its whole range, while chrono is still a
/// workspace dependency.
mod against_chrono {
    use chrono::{DateTime, NaiveDate, SecondsFormat};

    use super::super::{date, date_time, rfc3339};

    /// Days from 1970: dense near the era and leap-year boundaries, sparse elsewhere.
    fn days() -> Vec<i64> {
        let mut days: Vec<i64> = (-800_000..=800_000).step_by(997).collect();
        for anchor in [
            -96_465_292,
            -2_440_550,
            -719_469,
            -719_162,
            -25_508,
            0,
            10_957,
            11_016,
            157_113,
            2_932_897,
            95_026_236,
        ] {
            days.extend(anchor - 3..=anchor + 3);
        }
        days.extend((-96_465_300..=95_026_240).step_by(104_729));
        days
    }

    #[test]
    fn every_date_is_chronos_text_or_none_where_chrono_had_none() {
        let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).expect("the epoch");
        for days in days() {
            let theirs = epoch
                .checked_add_signed(chrono::Duration::days(days))
                .map(|day| day.to_string());
            assert_eq!(date(days), theirs, "{days}");
        }
    }

    #[test]
    fn every_date_time_is_chronos_text_or_none_where_chrono_had_none() {
        for days in days() {
            for (of_day, nanosecond) in [
                (0, 0),
                (1, 1_000),
                (43_200, 500_000_000),
                (86_399, 999_999_999),
                (37_800, 123_456_000),
            ] {
                let second = days * 86_400 + of_day;
                let theirs = DateTime::from_timestamp(second, nanosecond);
                assert_eq!(
                    date_time(second, nanosecond),
                    theirs.map(|at| at.naive_utc().format("%Y-%m-%dT%H:%M:%S%.f").to_string()),
                    "{second} {nanosecond}"
                );
                assert_eq!(
                    rfc3339(second, nanosecond),
                    theirs.map(|at| at.to_rfc3339_opts(SecondsFormat::AutoSi, false)),
                    "{second} {nanosecond}"
                );
            }
        }
    }
}
