//! The calendar a `Date` and a date-time's date are checked on: every year from 0000 to
//! 9999 is the proleptic Gregorian one, leap days included, so 0000-02-29 and 2000-02-29
//! exist and 1900-02-29 does not.

use super::Temporal;

#[test]
fn leap_days_exist_in_years_divisible_by_400_and_by_4_but_not_100() {
    for text in [
        "0000-02-29",
        "0004-02-29",
        "1600-02-29",
        "2000-02-29",
        "2024-02-29",
        "9996-02-29",
    ] {
        assert_eq!(Temporal::Date.refusal(text), None, "{text}");
    }
    for text in [
        "0100-02-29",
        "1900-02-29",
        "2100-02-29",
        "2023-02-29",
        "9999-02-29",
    ] {
        assert_eq!(Temporal::Date.refusal(text), Some("is not one"), "{text}");
    }
}

#[test]
fn every_year_from_0000_to_9999_has_its_first_and_last_day() {
    for text in ["0000-01-01", "0000-12-31", "9999-01-01", "9999-12-31"] {
        assert_eq!(Temporal::Date.refusal(text), None, "{text}");
    }
    for text in [
        "2026-04-31",
        "2026-06-31",
        "2026-09-31",
        "2026-11-31",
        "2026-00-10",
        "2026-01-00",
    ] {
        assert_eq!(Temporal::Date.refusal(text), Some("is not one"), "{text}");
    }
}
