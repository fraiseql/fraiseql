//! Reading text: the relaxed RFC 3339 grammar chrono's `DateTime` parser accepted.
//!
//! `date (T|t|space) time [whitespace] offset`, where:
//! - each number may be preceded by whitespace; a year is 1 to 4 digits, or a sign and any number
//!   of digits; month, day, hour, minute and second are 1 or 2 digits;
//! - whitespace is allowed around the `-` and `:` separators;
//! - the second may be 60, read as the first second of the next minute;
//! - a fraction is `.` and at least one digit; digits past the ninth are ignored;
//! - the offset is `Z`, `z`, `UTC` in any case, or a sign (`+`, `-`, or U+2212) with two-digit
//!   hours and two-digit minutes, separated by any run of `:` and whitespace;
//! - whitespace may follow.
//!
//! The strict grammar is the one chrono's `DateTime::parse_from_rfc3339` held text to:
//! `YYYY-MM-DD (T|t|space) hh:mm:ss[.fraction] (Z|z|±hh:mm)`, every number its full
//! width, no other whitespace, the sign `+`, `-` or U+2212. A leap second (`:60`) is
//! refused: chrono read it, and no instant it read that way converted to a `Timestamp`.

use jiff::{SignedDuration, civil, tz::Offset};

use crate::Timestamp;

/// The instant `text` names, or `None` when it is not one a [`Timestamp`] holds.
pub fn parse(text: &str) -> Option<Timestamp> {
    read(text, false)
}

/// The instant `text` names in the strict RFC 3339 grammar, or `None`.
pub fn parse_strict(text: &str) -> Option<Timestamp> {
    let f = strict_fields(text)?;
    if f.second > 59 {
        return None;
    }
    instant(f.year, f.month, f.day, f.hour, f.minute, f.second, f.nanosecond, f.offset)
}

/// The fields of a date and time as text spells them, not yet checked against the
/// calendar.
pub struct Fields {
    pub year:       i64,
    pub month:      i64,
    pub day:        i64,
    pub hour:       i64,
    pub minute:     i64,
    pub second:     i64,
    pub nanosecond: i64,
    /// Seconds east of UTC.
    pub offset:     i32,
}

/// The fields of `text` in the strict RFC 3339 grammar, whose second may be 60.
pub fn strict_fields(text: &str) -> Option<Fields> {
    let mut s = Cursor(text);
    let year = s.exact(4)?;
    s.literal('-')?;
    let month = s.exact(2)?;
    s.literal('-')?;
    let day = s.exact(2)?;
    s.separator()?;
    let hour = s.exact(2)?;
    s.literal(':')?;
    let minute = s.exact(2)?;
    s.literal(':')?;
    let second = s.exact(2)?;
    let nanosecond = s.fraction()?;
    let offset = s.strict_offset()?;
    s.0.is_empty().then_some(Fields {
        year,
        month,
        day,
        hour,
        minute,
        second,
        nanosecond,
        offset,
    })
}

/// `(year, month, day)` in chrono's `%Y-%m-%d` format grammar: a year as
/// [`Cursor::year`] reads it, a month and a day of one or two digits, whitespace before
/// each number and nowhere else.
pub fn format_date(text: &str) -> Option<(i64, i64, i64)> {
    let mut s = Cursor(text);
    let date = s.format_date()?;
    s.0.is_empty().then_some(date)
}

/// The fields of `text` in chrono's `%Y-%m-%dT%H:%M:%S` format grammar: [`format_date`],
/// an upper-case `T`, and an hour, minute and second of one or two digits, whitespace
/// before each. No fraction and no offset.
pub fn format_date_time(text: &str) -> Option<Fields> {
    let mut s = Cursor(text);
    let (year, month, day) = s.format_date()?;
    s.literal('T')?;
    let hour = s.number(2)?;
    s.literal(':')?;
    let minute = s.number(2)?;
    s.literal(':')?;
    let second = s.number(2)?;
    s.0.is_empty().then_some(Fields {
        year,
        month,
        day,
        hour,
        minute,
        second,
        nanosecond: 0,
        offset: 0,
    })
}

/// PostgreSQL's text form of a `timestamptz` (`2025-11-20 17:30:00.12+05:30`), whose
/// offset may be hours alone (`+00`).
#[cfg(feature = "sqlx")]
pub fn parse_pg_text(text: &str) -> Option<Timestamp> {
    read(text, true)
}

fn read(text: &str, hours_only_offset: bool) -> Option<Timestamp> {
    let mut s = Cursor(text);
    let year = s.year()?;
    s.space();
    s.literal('-')?;
    let month = s.number(2)?;
    s.space();
    s.literal('-')?;
    let day = s.number(2)?;
    s.separator()?;
    let hour = s.number(2)?;
    s.space();
    s.literal(':')?;
    let minute = s.number(2)?;
    s.space();
    s.literal(':')?;
    let second = s.number(2)?;
    let nanosecond = s.fraction()?;
    s.space();
    let offset = s.offset(hours_only_offset)?;
    s.space();
    if !s.0.is_empty() || second > 60 {
        return None;
    }
    instant(year, month, day, hour, minute, second, nanosecond, offset)
}

/// The instant the fields name; a `second` of 60 is the first second of the next minute.
#[allow(clippy::too_many_arguments)] // Reason: the fields of one date-time, read in order
fn instant(
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    nanosecond: i64,
    offset: i32,
) -> Option<Timestamp> {
    let date = civil::Date::new(
        i16::try_from(year).ok()?,
        i8::try_from(month).ok()?,
        i8::try_from(day).ok()?,
    )
    .ok()?;
    let time = civil::Time::new(
        i8::try_from(hour).ok()?,
        i8::try_from(minute).ok()?,
        i8::try_from(second.min(59)).ok()?,
        i32::try_from(nanosecond).ok()?,
    )
    .ok()?;
    let instant = Offset::from_seconds(offset).ok()?.to_timestamp(date.to_datetime(time)).ok()?;
    let instant = if second == 60 {
        instant.checked_add(SignedDuration::from_secs(1)).ok()?
    } else {
        instant
    };
    Some(Timestamp(instant))
}

struct Cursor<'a>(&'a str);

impl Cursor<'_> {
    fn space(&mut self) {
        self.0 = self.0.trim_start();
    }

    fn literal(&mut self, c: char) -> Option<()> {
        self.0 = self.0.strip_prefix(c)?;
        Some(())
    }

    /// One to `width` ASCII digits, after any whitespace.
    fn number(&mut self, width: usize) -> Option<i64> {
        self.space();
        self.digits(width)
    }

    fn digits(&mut self, width: usize) -> Option<i64> {
        let len = self.0.bytes().take(width).take_while(u8::is_ascii_digit).count();
        if len == 0 {
            return None;
        }
        let (digits, rest) = self.0.split_at(len);
        self.0 = rest;
        digits.parse().ok()
    }

    /// Exactly `width` ASCII digits.
    fn exact(&mut self, width: usize) -> Option<i64> {
        let digits = self.0.get(..width).filter(|d| d.bytes().all(|b| b.is_ascii_digit()))?;
        self.0 = &self.0[width..];
        digits.parse().ok()
    }

    fn year(&mut self) -> Option<i64> {
        self.space();
        if let Some(rest) = self.0.strip_prefix('-') {
            self.0 = rest;
            self.digits(usize::MAX).map(|year| -year)
        } else if let Some(rest) = self.0.strip_prefix('+') {
            self.0 = rest;
            self.digits(usize::MAX)
        } else {
            self.digits(4)
        }
    }

    fn format_date(&mut self) -> Option<(i64, i64, i64)> {
        let year = self.year()?;
        self.literal('-')?;
        let month = self.number(2)?;
        self.literal('-')?;
        let day = self.number(2)?;
        Some((year, month, day))
    }

    fn separator(&mut self) -> Option<()> {
        let rest = self.0.strip_prefix(['T', 't', ' '])?;
        self.0 = rest;
        Some(())
    }

    /// `.` and one or more digits, as nanoseconds; nothing when there is no `.`.
    fn fraction(&mut self) -> Option<i64> {
        let Some(rest) = self.0.strip_prefix('.') else {
            return Some(0);
        };
        self.0 = rest;
        let len = self.0.bytes().take(9).take_while(u8::is_ascii_digit).count();
        let value = self.digits(9)?;
        self.0 = self.0.trim_start_matches(|c: char| c.is_ascii_digit());
        let scale = 10_i64.pow(u32::try_from(9 - len).ok()?);
        Some(value * scale)
    }

    /// The offset in seconds east of UTC; minutes may be left out when
    /// `hours_only` is set.
    fn offset(&mut self, hours_only: bool) -> Option<i32> {
        if self.0.get(..3).is_some_and(|utc| utc.eq_ignore_ascii_case("UTC")) {
            self.0 = &self.0[3..];
            return Some(0);
        }
        if let Some(rest) = self.0.strip_prefix(['Z', 'z']) {
            self.0 = rest;
            return Some(0);
        }
        let negative = if let Some(rest) = self.0.strip_prefix('+') {
            self.0 = rest;
            false
        } else if let Some(rest) = self.0.strip_prefix(['-', '\u{2212}']) {
            self.0 = rest;
            true
        } else {
            return None;
        };
        let hours = self.two_digits(b'9')?;
        self.0 = self.0.trim_start_matches(|c: char| c == ':' || c.is_whitespace());
        let minutes = if hours_only && !self.0.as_bytes().first().is_some_and(u8::is_ascii_digit) {
            0
        } else {
            self.two_digits(b'5')?
        };
        let seconds = hours * 3600 + minutes * 60;
        if seconds >= 86_400 {
            return None;
        }
        Some(if negative { -seconds } else { seconds })
    }

    /// `Z`, `z`, or a sign and `hh:mm`, as seconds east of UTC.
    fn strict_offset(&mut self) -> Option<i32> {
        if let Some(rest) = self.0.strip_prefix(['Z', 'z']) {
            self.0 = rest;
            return Some(0);
        }
        let negative = if let Some(rest) = self.0.strip_prefix('+') {
            self.0 = rest;
            false
        } else if let Some(rest) = self.0.strip_prefix(['-', '\u{2212}']) {
            self.0 = rest;
            true
        } else {
            return None;
        };
        let hours = self.two_digits(b'9')?;
        self.literal(':')?;
        let minutes = self.two_digits(b'5')?;
        let seconds = hours * 3600 + minutes * 60;
        if seconds >= 86_400 {
            return None;
        }
        Some(if negative { -seconds } else { seconds })
    }

    /// Exactly two ASCII digits, the first no greater than `first_max`.
    fn two_digits(&mut self, first_max: u8) -> Option<i32> {
        match self.0.as_bytes() {
            [a @ b'0'..=b'9', b @ b'0'..=b'9', ..] if *a <= first_max => {
                self.0 = &self.0[2..];
                Some(i32::from(a - b'0') * 10 + i32::from(b - b'0'))
            },
            _ => None,
        }
    }
}
