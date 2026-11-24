//! Reading dates and date-times in the grammars chrono's parsers held text to, over the
//! years chrono held, to days or seconds from the Unix epoch rather than to a
//! [`Timestamp`](crate::Timestamp), which holds only years -9999 to 9999.

use super::{MAX_YEAR, MIN_YEAR, days_from_civil};
use crate::parse::{self, Fields};

/// Days from 1970-01-01 to `year-month-day`, or `None` when it is not a date of chrono's
/// years.
fn checked_days(year: i64, month: i64, day: i64) -> Option<i64> {
    if !(MIN_YEAR..=MAX_YEAR).contains(&year) || !(1..=12).contains(&month) || day < 1 {
        return None;
    }
    let first = days_from_civil(year, month, 1);
    let next = if month == 12 {
        days_from_civil(year + 1, 1, 1)
    } else {
        days_from_civil(year, month + 1, 1)
    };
    (day <= next - first).then_some(first + day - 1)
}

/// The instant the fields name, as whole seconds from the Unix epoch and nanoseconds. A
/// second of 60 is the first second of the next minute, as chrono read it.
fn checked_instant(f: &Fields) -> Option<(i64, u32)> {
    if f.hour > 23 || f.minute > 59 || f.second > 60 {
        return None;
    }
    let days = checked_days(f.year, f.month, f.day)?;
    let second = days * 86_400 + f.hour * 3_600 + f.minute * 60 + f.second - i64::from(f.offset);
    Some((second, u32::try_from(f.nanosecond).ok()?))
}

/// The days from 1970-01-01 to the date `text` names in chrono's `%Y-%m-%d` grammar:
/// `2024-01-15`, `2024-1-5`, ` 2024-01- 15`, `-0044-03-15`, `+10000-01-01`.
///
/// A year is one to four digits, or a sign and any number of digits; a month and a day
/// are one or two digits; whitespace may come before each number and nowhere else. `None`
/// for any other text, or a date outside the years chrono held.
#[must_use]
pub fn read_date(text: &str) -> Option<i64> {
    let (year, month, day) = parse::format_date(text)?;
    checked_days(year, month, day)
}

/// The instant `text` names in chrono's `%Y-%m-%dT%H:%M:%S` grammar, read as UTC: whole
/// seconds from the Unix epoch, and nanoseconds, which are 0.
///
/// The date is [`read_date`]'s; then an upper-case `T`, and an hour, minute and second of
/// one or two digits, each after any whitespace. A second of 60 is the first second of the
/// next minute. No fraction and no offset.
#[must_use]
pub fn read_date_time(text: &str) -> Option<(i64, u32)> {
    checked_instant(&parse::format_date_time(text)?)
}

/// The instant `text` names in RFC 3339, as chrono's `DateTime::parse_from_rfc3339` read
/// it: whole seconds from the Unix epoch (floored), and nanoseconds.
///
/// `YYYY-MM-DD (T|t|space) hh:mm:ss[.fraction] (Z|z|±hh:mm)`, every number its full width,
/// the sign `+`, `-` or U+2212, digits of the fraction past the ninth ignored. A second of
/// 60 is the first second of the next minute. Unlike
/// [`Timestamp::parse_rfc3339_strict`](crate::Timestamp::parse_rfc3339_strict), every year
/// from 0000 to 9999 is read, whatever its offset.
#[must_use]
pub fn read_rfc3339(text: &str) -> Option<(i64, u32)> {
    checked_instant(&parse::strict_fields(text)?)
}

#[cfg(test)]
#[path = "calendar_read_tests.rs"]
mod tests;
