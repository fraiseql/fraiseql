//! Instants the server writes as text, or computes from a configured or stored length,
//! as it did when they were chrono's.

use std::fmt::Write as _;

use fraiseql_time::{Timestamp, calendar, jiff::SignedDuration};
use sqlx::{Row as _, postgres::PgRow};

/// The last instant a [`Timestamp`] holds, 9999-12-30T22:00:00.999999999Z.
pub fn last() -> Timestamp {
    fraiseql_time::jiff::Timestamp::MAX.into()
}

/// Whole seconds from the Unix epoch (floored) and nanoseconds.
fn parts(at: Timestamp) -> (i64, u32) {
    let nanos = at.as_jiff().as_nanosecond();
    (
        i64::try_from(nanos.div_euclid(1_000_000_000)).expect("a Timestamp's seconds fit an i64"),
        u32::try_from(nanos.rem_euclid(1_000_000_000)).expect("a remainder of 10^9 fits a u32"),
    )
}

/// `second`'s date and time of day, `2024-01-15T10:30:00`.
fn whole_second(second: i64) -> String {
    calendar::date_time(second, 0).expect("a Timestamp's date is within the calendar's years")
}

/// `2024-01-15T10:30:00.123Z`: milliseconds always, truncated, and `Z`, as chrono's
/// `to_rfc3339_opts(SecondsFormat::Millis, true)` wrote it.
pub fn rfc3339_millis_z(at: Timestamp) -> String {
    let (second, nanosecond) = parts(at);
    let mut text = whole_second(second);
    // Writing to a `String` cannot fail.
    let _ = write!(text, ".{:03}Z", nanosecond / 1_000_000);
    text
}

/// `2024-01-15T10:30:00Z`: whole seconds, truncated, and `Z`, as chrono's
/// `to_rfc3339_opts(SecondsFormat::Secs, true)` wrote it.
#[cfg(feature = "sources")]
pub fn rfc3339_seconds_z(at: Timestamp) -> String {
    let mut text = whole_second(parts(at).0);
    text.push('Z');
    text
}

/// `2024-01`, as chrono's `format("%Y-%m")` wrote it.
pub fn year_month(at: Timestamp) -> String {
    let mut date = whole_second(parts(at).0);
    date.truncate(date.find('-').map_or(0, |year| year + 3));
    date
}

/// `now + secs`, or the last instant when that is later. A length longer than any
/// instant can carry keeps its subject until the last one.
pub fn later_by(now: Timestamp, secs: u64) -> Timestamp {
    i64::try_from(secs)
        .ok()
        .and_then(|secs| now.checked_add(SignedDuration::from_secs(secs)))
        .unwrap_or_else(last)
}

/// A time bound in RFC 3339, read as chrono's `parse_from_rfc3339` read it (a leap second
/// included), or the last instant when it names a later one. `None` when `text` is not
/// RFC 3339.
pub fn rfc3339_bound(text: &str) -> Option<Timestamp> {
    let (second, nanosecond) = calendar::read_rfc3339(text)?;
    let nanosecond = i32::try_from(nanosecond).ok()?;
    Some(Timestamp::new(second, nanosecond).unwrap_or_else(|_| last()))
}

/// An expiry stored in `column`, read as the last instant when it is later than that (a
/// row written when an expiry could reach past year 9999). That only ever shortens an
/// expiry. Any other error is returned.
///
/// # Errors
///
/// Returns the read's error when it is not the value's decode.
pub fn stored_expiry(row: &PgRow, column: &str) -> Result<Option<Timestamp>, sqlx::Error> {
    match row.try_get(column) {
        Err(sqlx::Error::ColumnDecode { .. }) => Ok(Some(last())),
        read => read,
    }
}

#[cfg(test)]
#[path = "time_text_tests.rs"]
mod tests;
