//! The instant type FraiseQL crates share.
//!
//! [`Timestamp`] is a point in time, held as a [`jiff::Timestamp`]. What it adds is fixed
//! bytes for every form in which an instant leaves the process:
//!
//! - **Text.** [`Timestamp`]'s serde form, [`Display`](std::fmt::Display),
//!   [`Debug`](std::fmt::Debug) and [`Timestamp::to_rfc3339`] write the bytes chrono's
//!   `DateTime<Utc>` wrote for the same instant. A fraction of a second is padded to 3, 6 or 9
//!   digits (`.120`, never `.12`), and left out at a whole second:
//!
//!   | form | example |
//!   |---|---|
//!   | serde, `Debug` | `2025-11-20T12:00:00.120Z` |
//!   | `to_rfc3339` | `2025-11-20T12:00:00.120+00:00` |
//!   | `Display` | `2025-11-20 12:00:00.120 UTC` |
//!
//!   Stored documents, signed payloads, idempotency keys and hash chains are compared
//!   byte for byte, so an equal instant must always be the equal string.
//!
//! - **PostgreSQL.** With the `sqlx` or `postgres` feature, a [`Timestamp`] is a `timestamptz`:
//!   whole microseconds from 2000-01-01T00:00:00Z. Writing drops anything below a microsecond,
//!   toward 2000-01-01. Reading a value outside the range a [`Timestamp`] holds (`infinity`,
//!   `-infinity`, or a year past 9999) is a decode error naming the value.
//!
//! - **chrono.** With the `chrono` feature, a [`Timestamp`] converts into a
//!   `chrono::DateTime<Utc>`, and one converts back with `TryFrom`, which fails outside the range a
//!   [`Timestamp`] holds.
//!
//! The type keeps those forms to itself: it has no `Deref` to [`jiff::Timestamp`], so
//! jiff's own text forms are reached only through [`Timestamp::as_jiff`].

use std::{
    fmt,
    ops::{Add, AddAssign, Sub, SubAssign},
    str::FromStr,
};

use jiff::{SignedDuration, tz::Offset};

#[cfg(feature = "chrono")]
mod chrono;
mod parse;

/// The jiff this crate's public API is built against.
pub use jiff;
#[cfg(any(feature = "sqlx", feature = "postgres"))]
mod pg;
#[cfg(feature = "postgres")]
mod postgres;
#[cfg(feature = "sqlx")]
mod sqlx;

/// An instant, with chrono-compatible text and a microsecond `timestamptz`.
///
/// See the [crate documentation](crate) for the forms it writes and reads.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(jiff::Timestamp);

/// A value that is not an instant a [`Timestamp`] can hold.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The text is not an RFC 3339 timestamp with an offset.
    #[error("not an RFC 3339 timestamp: {input:?}")]
    Parse {
        /// The text that was read.
        input: String,
    },
    /// The value lies outside the range a [`Timestamp`] holds.
    #[error("{value} is outside the range of a timestamp (-9999-01-02 to 9999-12-30)")]
    OutOfRange {
        /// The value, as it was given.
        value: String,
    },
}

impl Timestamp {
    /// The current instant, from the system clock.
    #[must_use]
    pub fn now() -> Self {
        Self(jiff::Timestamp::now())
    }

    /// The instant `second` seconds and `nanosecond` nanoseconds from the Unix epoch.
    ///
    /// # Errors
    ///
    /// Returns [`Error::OutOfRange`] when the instant is outside the range a
    /// [`Timestamp`] holds, or `nanosecond` is not below one second in magnitude.
    pub fn new(second: i64, nanosecond: i32) -> Result<Self, Error> {
        jiff::Timestamp::new(second, nanosecond)
            .map(Self)
            .map_err(|_| Error::OutOfRange {
                value: format!("{second} s {nanosecond} ns from 1970-01-01T00:00:00Z"),
            })
    }

    /// The instant `second` whole seconds from the Unix epoch.
    ///
    /// # Errors
    ///
    /// Returns [`Error::OutOfRange`] when the instant is outside the range.
    pub fn from_second(second: i64) -> Result<Self, Error> {
        Self::new(second, 0)
    }

    /// The instant `millisecond` milliseconds from the Unix epoch.
    ///
    /// # Errors
    ///
    /// Returns [`Error::OutOfRange`] when the instant is outside the range.
    pub fn from_millisecond(millisecond: i64) -> Result<Self, Error> {
        jiff::Timestamp::from_millisecond(millisecond)
            .map(Self)
            .map_err(|_| Error::OutOfRange {
                value: format!("{millisecond} ms from 1970-01-01T00:00:00Z"),
            })
    }

    /// The instant `microsecond` microseconds from the Unix epoch.
    ///
    /// # Errors
    ///
    /// Returns [`Error::OutOfRange`] when the instant is outside the range.
    pub fn from_microsecond(microsecond: i64) -> Result<Self, Error> {
        jiff::Timestamp::from_microsecond(microsecond)
            .map(Self)
            .map_err(|_| Error::OutOfRange {
                value: format!("{microsecond} µs from 1970-01-01T00:00:00Z"),
            })
    }

    /// The same instant as a [`jiff::Timestamp`], for time zone and calendar work.
    ///
    /// The result writes jiff's text forms, not this type's.
    #[must_use]
    pub const fn as_jiff(self) -> jiff::Timestamp {
        self.0
    }

    /// Whole seconds from the Unix epoch, truncated toward zero.
    #[must_use]
    pub fn as_second(self) -> i64 {
        self.0.as_second()
    }

    /// Whole milliseconds from the Unix epoch, truncated toward zero.
    #[must_use]
    pub fn as_millisecond(self) -> i64 {
        self.0.as_millisecond()
    }

    /// Whole microseconds from the Unix epoch, truncated toward zero.
    #[must_use]
    pub fn as_microsecond(self) -> i64 {
        self.0.as_microsecond()
    }

    /// The fractional second in nanoseconds, with the sign of [`Self::as_second`].
    #[must_use]
    pub fn subsec_nanosecond(self) -> i32 {
        self.0.subsec_nanosecond()
    }

    /// `self + duration`, or `None` when the result is outside the range.
    #[must_use]
    pub fn checked_add(self, duration: SignedDuration) -> Option<Self> {
        self.0.checked_add(duration).ok().map(Self)
    }

    /// `self - duration`, or `None` when the result is outside the range.
    #[must_use]
    pub fn checked_sub(self, duration: SignedDuration) -> Option<Self> {
        self.0.checked_sub(duration).ok().map(Self)
    }

    /// The time from `earlier` to `self`; negative when `earlier` is later.
    #[must_use]
    pub fn duration_since(self, earlier: Self) -> SignedDuration {
        self.0.duration_since(earlier.0)
    }

    /// The instant as RFC 3339 with a `+00:00` offset: `2025-11-20T12:00:00.120+00:00`.
    #[must_use]
    pub fn to_rfc3339(self) -> String {
        format!("{}+00:00", Civil(self, 'T'))
    }

    /// Read an RFC 3339 timestamp with any offset, as chrono's `DateTime` parser read it:
    /// `2025-11-20T12:00:00.12+02:00`, with `T`, `t` or a space before the time, and `Z`,
    /// `z`, `UTC` or `±HH:MM` for the offset. Serde reads the same text.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Parse`] when `text` is not such a timestamp or names an instant
    /// outside the range a [`Timestamp`] holds.
    pub fn parse_rfc3339(text: &str) -> Result<Self, Error> {
        text.parse()
    }
}

impl FromStr for Timestamp {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Error> {
        parse::parse(text).ok_or_else(|| Error::Parse {
            input: text.to_owned(),
        })
    }
}

impl From<jiff::Timestamp> for Timestamp {
    fn from(instant: jiff::Timestamp) -> Self {
        Self(instant)
    }
}

impl From<Timestamp> for jiff::Timestamp {
    fn from(instant: Timestamp) -> Self {
        instant.0
    }
}

/// The calendar date and time in UTC, `separator` between them, chrono's fraction rule.
struct Civil(Timestamp, char);

impl fmt::Display for Civil {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let dt = Offset::UTC.to_datetime(self.0.0);
        let year = dt.year();
        if (0..=9999).contains(&year) {
            write!(f, "{year:04}")?;
        } else {
            write!(f, "{year:+05}")?;
        }
        write!(
            f,
            "-{:02}-{:02}{}{:02}:{:02}:{:02}",
            dt.month(),
            dt.day(),
            self.1,
            dt.hour(),
            dt.minute(),
            dt.second()
        )?;
        let nanos = dt.subsec_nanosecond();
        if nanos == 0 {
            Ok(())
        } else if nanos % 1_000_000 == 0 {
            write!(f, ".{:03}", nanos / 1_000_000)
        } else if nanos % 1_000 == 0 {
            write!(f, ".{:06}", nanos / 1_000)
        } else {
            write!(f, ".{nanos:09}")
        }
    }
}

/// `2025-11-20 12:00:00.120 UTC`.
impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} UTC", Civil(*self, ' '))
    }
}

/// `2025-11-20T12:00:00.120Z`, the serde form.
impl fmt::Debug for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}Z", Civil(*self, 'T'))
    }
}

impl serde::Serialize for Timestamp {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&format_args!("{self:?}"))
    }
}

impl<'de> serde::Deserialize<'de> for Timestamp {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = Timestamp;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an RFC 3339 timestamp")
            }

            fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<Timestamp, E> {
                text.parse().map_err(E::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

/// Panics when the result is outside the range, as chrono's `+` did; see
/// [`Timestamp::checked_add`].
impl Add<SignedDuration> for Timestamp {
    type Output = Self;

    fn add(self, duration: SignedDuration) -> Self {
        Self(self.0 + duration)
    }
}

/// Panics when the result is outside the range; see [`Timestamp::checked_sub`].
impl Sub<SignedDuration> for Timestamp {
    type Output = Self;

    fn sub(self, duration: SignedDuration) -> Self {
        Self(self.0 - duration)
    }
}

impl AddAssign<SignedDuration> for Timestamp {
    fn add_assign(&mut self, duration: SignedDuration) {
        *self = *self + duration;
    }
}

impl SubAssign<SignedDuration> for Timestamp {
    fn sub_assign(&mut self, duration: SignedDuration) {
        *self = *self - duration;
    }
}

/// The time from `earlier` to `self`, as [`Timestamp::duration_since`].
impl Sub for Timestamp {
    type Output = SignedDuration;

    fn sub(self, earlier: Self) -> SignedDuration {
        self.duration_since(earlier)
    }
}

#[cfg(test)]
mod tests;
