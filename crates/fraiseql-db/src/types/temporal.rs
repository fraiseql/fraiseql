//! The value spaces of the four temporal scalars, as the GraphQL scalars they publish
//! specify them (<https://scalars.graphql.org/chillicream/>):
//!
//! | Scalar | Written | Offset |
//! |---|---|---|
//! | `DateTime` | `YYYY-MM-DDTHH:MM:SS[.f]` then `Z` or `±HH:MM` | required, never `-00:00` |
//! | `LocalDateTime` | `YYYY-MM-DDTHH:MM:SS[.f]` | never |
//! | `Date` | `YYYY-MM-DD` | never |
//! | `Time` | `HH:MM:SS[.f]` | never |
//!
//! `T` and `Z` may be lower case, the fraction `.f` is one to nine digits, and the date and
//! the time of day must exist (no 30 February, no hour 24, no second 60).
//!
//! An operand is cast `::timestamptz`, `::timestamp`, `::date` or `::time`, and PostgreSQL's
//! input parser reads far more than these: an offset-less instant in the session's zone, a
//! `::timestamp` or `::time` operand with its offset dropped, words such as `now`. Each of
//! those compares a value other than the one sent, so it is refused here instead.

use fraiseql_error::{FraiseQLError, Result};
use fraiseql_time::jiff::civil;

/// A temporal scalar whose value space this module knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Temporal {
    /// An instant: a date and time of day with its offset from UTC.
    DateTime,
    /// A date and time of day in no time zone.
    LocalDateTime,
    /// A calendar date.
    Date,
    /// A time of day in no time zone.
    Time,
}

const NOT_ONE: &str = "is not one";

impl Temporal {
    /// The scalar's name and the shape it is written in, completing "`x` takes …".
    const fn described(self) -> &'static str {
        match self {
            Self::DateTime => {
                "a DateTime, a date and time of day with its offset from UTC \
                 (YYYY-MM-DDTHH:MM:SS, optional fractional seconds, then Z or ±HH:MM)"
            },
            Self::LocalDateTime => {
                "a LocalDateTime, a date and time of day in no time zone \
                 (YYYY-MM-DDTHH:MM:SS, optional fractional seconds)"
            },
            Self::Date => "a Date (YYYY-MM-DD)",
            Self::Time => {
                "a Time, a time of day in no time zone (HH:MM:SS, optional fractional seconds)"
            },
        }
    }

    /// Why `text` is not a value of this scalar, or `None` when it is one.
    ///
    /// The reason is a phrase that completes "the value …", and never quotes the value.
    #[must_use]
    pub fn refusal(self, text: &str) -> Option<&'static str> {
        let b = text.as_bytes();
        let end = match self {
            Self::Date => date(b),
            Self::Time => time(b, 0),
            Self::DateTime | Self::LocalDateTime => date(b)
                .filter(|_| matches!(b.get(10), Some(b'T' | b't')))
                .and_then(|_| time(b, 11)),
        };
        let Some(end) = end else {
            return Some(NOT_ONE);
        };
        match (self, offset(&b[end..])) {
            (Self::DateTime, Offset::Known)
            | (Self::LocalDateTime | Self::Date | Self::Time, Offset::None) => None,
            (Self::DateTime, Offset::None) => {
                Some("has no time-zone offset, which a DateTime always carries")
            },
            (Self::DateTime, Offset::Unknown) => {
                Some("carries the unknown offset -00:00, which a DateTime does not take")
            },
            (Self::LocalDateTime, Offset::Known | Offset::Unknown) => {
                Some("carries a time-zone offset, which a LocalDateTime never has")
            },
            (Self::Time, Offset::Known | Offset::Unknown) => {
                Some("carries a time-zone offset, which a Time never has")
            },
            (_, Offset::Other) | (Self::Date, Offset::Known | Offset::Unknown) => Some(NOT_ONE),
        }
    }

    /// Refuse an operand compared against this scalar that is not one of its values,
    /// naming `name`.
    ///
    /// `null` passes (a null test is not a comparison), and a list is checked item by item,
    /// as an `in` operand is.
    ///
    /// # Errors
    ///
    /// [`FraiseQLError::Validation`] when the operand, or an item of it, is not a string
    /// holding a value of this scalar.
    pub fn check_operand(self, name: &str, value: &serde_json::Value) -> Result<()> {
        let refusal = match value {
            serde_json::Value::Null => None,
            serde_json::Value::Array(items) => {
                return items.iter().try_for_each(|item| self.check_operand(name, item));
            },
            serde_json::Value::String(text) => self.refusal(text),
            _ => Some(NOT_ONE),
        };
        refusal.map_or(Ok(()), |why| {
            Err(FraiseQLError::validation(format!(
                "`{name}` takes {}: the value {why}",
                self.described()
            )))
        })
    }
}

/// The value of the `len` ASCII digits at `at`, when they are all digits.
fn number(b: &[u8], at: usize, len: usize) -> Option<u32> {
    let digits = b.get(at..at + len)?;
    digits
        .iter()
        .try_fold(0_u32, |n, d| d.is_ascii_digit().then(|| n * 10 + u32::from(d - b'0')))
}

/// `YYYY-MM-DD` at the start of `b`: the index after it, when that date exists.
fn date(b: &[u8]) -> Option<usize> {
    let (year, month, day) = (number(b, 0, 4)?, number(b, 5, 2)?, number(b, 8, 2)?);
    if b.get(4) != Some(&b'-') || b.get(7) != Some(&b'-') {
        return None;
    }
    civil::Date::new(i16::try_from(year).ok()?, i8::try_from(month).ok()?, i8::try_from(day).ok()?)
        .ok()?;
    Some(10)
}

/// `HH:MM:SS`, then one to nine fraction digits after a `.`, at `at`: the index after it,
/// when that time of day exists.
fn time(b: &[u8], at: usize) -> Option<usize> {
    let (hour, minute, second) = (number(b, at, 2)?, number(b, at + 3, 2)?, number(b, at + 6, 2)?);
    if b.get(at + 2) != Some(&b':') || b.get(at + 5) != Some(&b':') {
        return None;
    }
    civil::Time::new(
        i8::try_from(hour).ok()?,
        i8::try_from(minute).ok()?,
        i8::try_from(second).ok()?,
        0,
    )
    .ok()?;
    let mut end = at + 8;
    if b.get(end) == Some(&b'.') {
        let fraction = b[end + 1..].iter().take_while(|d| d.is_ascii_digit()).count();
        if !(1..=9).contains(&fraction) {
            return None;
        }
        end += 1 + fraction;
    }
    Some(end)
}

/// What follows a time of day.
enum Offset {
    /// Nothing.
    None,
    /// `Z`, `z` or `±HH:MM` of a real offset.
    Known,
    /// `-00:00`: RFC 3339's "offset unknown".
    Unknown,
    /// Anything else.
    Other,
}

fn offset(tail: &[u8]) -> Offset {
    match tail {
        [] => Offset::None,
        [b'Z' | b'z'] => Offset::Known,
        [sign @ (b'+' | b'-'), _, _, b':', _, _] => {
            match (number(tail, 1, 2), number(tail, 4, 2)) {
                (Some(0), Some(0)) if *sign == b'-' => Offset::Unknown,
                (Some(hours), Some(minutes)) if hours <= 23 && minutes <= 59 => Offset::Known,
                _ => Offset::Other,
            }
        },
        _ => Offset::Other,
    }
}

#[cfg(test)]
#[path = "temporal/calendar_tests.rs"]
mod calendar_tests;
#[cfg(test)]
mod tests;
