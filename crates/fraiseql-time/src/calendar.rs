//! Calendar text for dates and date-times past the years a [`Timestamp`](crate::Timestamp)
//! holds.
//!
//! PostgreSQL stores dates and timestamps far beyond year 9999, and an uploaded Arrow
//! column can carry them too. chrono rendered every one inside its own range (years
//! -262143 to 262142), and those strings reach clients, so the text here is chrono's, byte
//! for byte, over that whole range:
//!
//! - a year from 0 to 9999 in four digits, any other with its sign and at least four digits:
//!   `-0044-03-15`, `+10000-01-01`;
//! - a fraction of a second in 3, 6 or 9 digits, and none at a whole second.
//!
//! Outside chrono's range every function returns `None`, as chrono refused those values.
//! The calendar is the proleptic Gregorian one, with a year 0 (1 BC).

use std::fmt::Write as _;

/// The first and last year chrono's dates held.
const MIN_YEAR: i64 = -262_143;
const MAX_YEAR: i64 = 262_142;

/// Days from 1970-01-01 to `year-month-day`, `month` and `day` already in range.
///
/// Howard Hinnant's `days_from_civil`, on a 400-year era that starts on March 1st.
const fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The date `days` after 1970-01-01, as `(year, month, day)`: Hinnant's `civil_from_days`.
const fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

/// The first and last day chrono's dates held, in days from 1970-01-01.
const MIN_DAY: i64 = days_from_civil(MIN_YEAR, 1, 1);
const MAX_DAY: i64 = days_from_civil(MAX_YEAR, 12, 31);

/// `year-mm-dd` with chrono's year rule.
fn write_date(out: &mut String, days: i64) {
    let (year, month, day) = civil_from_days(days);
    // Writing to a `String` cannot fail.
    let _ = if (0..=9999).contains(&year) {
        write!(out, "{year:04}-{month:02}-{day:02}")
    } else {
        write!(out, "{year:+05}-{month:02}-{day:02}")
    };
}

/// `.fff`, `.ffffff` or `.fffffffff`: the fewest of 3, 6 or 9 digits that hold `nanosecond`
/// exactly, and nothing at zero.
fn write_fraction(out: &mut String, nanosecond: u32) {
    // Writing to a `String` cannot fail.
    let _ = if nanosecond == 0 {
        Ok(())
    } else if nanosecond.is_multiple_of(1_000_000) {
        write!(out, ".{:03}", nanosecond / 1_000_000)
    } else if nanosecond.is_multiple_of(1_000) {
        write!(out, ".{:06}", nanosecond / 1_000)
    } else {
        write!(out, ".{nanosecond:09}")
    };
}

/// The date `days` after 1970-01-01: `2024-01-15`, `-0044-03-15`, `+10000-01-01`.
///
/// `None` outside the years chrono held.
#[must_use]
pub fn date(days: i64) -> Option<String> {
    if !(MIN_DAY..=MAX_DAY).contains(&days) {
        return None;
    }
    let mut out = String::with_capacity(12);
    write_date(&mut out, days);
    Some(out)
}

/// The UTC date and time `second` seconds and `nanosecond` nanoseconds after the Unix
/// epoch, with a `T` and no offset: `2024-01-15T10:30:00.500`.
///
/// `second` is floored: the instant 1 µs before the epoch is `second = -1`,
/// `nanosecond = 999_999_000`. `None` when `nanosecond` is a second or more, or the date
/// is outside the years chrono held.
#[must_use]
pub fn date_time(second: i64, nanosecond: u32) -> Option<String> {
    if nanosecond >= 1_000_000_000 {
        return None;
    }
    let days = second.div_euclid(86_400);
    if !(MIN_DAY..=MAX_DAY).contains(&days) {
        return None;
    }
    let of_day = second.rem_euclid(86_400);
    let mut out = String::with_capacity(32);
    write_date(&mut out, days);
    // Writing to a `String` cannot fail.
    let _ = write!(out, "T{:02}:{:02}:{:02}", of_day / 3_600, of_day / 60 % 60, of_day % 60);
    write_fraction(&mut out, nanosecond);
    Some(out)
}

/// [`date_time`] as RFC 3339 in UTC, with chrono's `+00:00`:
/// `2024-01-15T10:30:00.500+00:00`.
#[must_use]
pub fn rfc3339(second: i64, nanosecond: u32) -> Option<String> {
    date_time(second, nanosecond).map(|mut text| {
        text.push_str("+00:00");
        text
    })
}

#[cfg(test)]
#[path = "calendar_tests.rs"]
mod tests;
