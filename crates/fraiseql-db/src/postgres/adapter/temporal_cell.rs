//! A `timestamptz`, `timestamp` or `date` cell as its response text, decoded from
//! PostgreSQL's binary form and written by [`fraiseql_time::calendar`].
//!
//! Not through a date-time type: PostgreSQL stores years far past 9999, and the response
//! has always carried them (`+10000-01-01`), down to the years chrono held. A value past
//! those, or `infinity` and `-infinity`, is a decode error, so the cell is refused by type
//! as before.

use std::error::Error;

use fraiseql_time::calendar;
use tokio_postgres::types::{FromSql, Type};

/// Microseconds from 1970-01-01 to 2000-01-01, PostgreSQL's epoch.
const PG_EPOCH_MICROS: i64 = 946_684_800_000_000;
/// Days from 1970-01-01 to 2000-01-01.
const PG_EPOCH_DAYS: i64 = 10_957;

type DecodeResult<T> = std::result::Result<T, Box<dyn Error + Sync + Send>>;

/// The text of a timestamp stored as `raw`, microseconds from 2000-01-01, by `write`.
fn instant_text(raw: &[u8], write: fn(i64, u32) -> Option<String>) -> DecodeResult<String> {
    let micros: [u8; 8] = raw.try_into().map_err(|_| "a timestamp is 8 bytes")?;
    let micros = i64::from_be_bytes(micros);
    let unix = micros.checked_add(PG_EPOCH_MICROS).ok_or("timestamp out of range")?;
    let nanosecond = u32::try_from(unix.rem_euclid(1_000_000) * 1_000)
        .map_err(|_| "a microsecond fraction is below a second")?;
    write(unix.div_euclid(1_000_000), nanosecond).ok_or_else(|| "timestamp out of range".into())
}

/// A `timestamptz` as RFC 3339 in UTC with `+00:00`: `2024-01-15T10:30:00.500+00:00`.
pub(super) struct PgTimestamptzText(pub(super) String);

impl<'a> FromSql<'a> for PgTimestamptzText {
    fn from_sql(_ty: &Type, raw: &'a [u8]) -> DecodeResult<Self> {
        instant_text(raw, calendar::rfc3339).map(Self)
    }

    fn accepts(ty: &Type) -> bool {
        *ty == Type::TIMESTAMPTZ
    }
}

/// A `timestamp` as a local date-time, `T` and no offset: `2024-01-15T10:30:00.500`.
pub(super) struct PgTimestampText(pub(super) String);

impl<'a> FromSql<'a> for PgTimestampText {
    fn from_sql(_ty: &Type, raw: &'a [u8]) -> DecodeResult<Self> {
        instant_text(raw, calendar::date_time).map(Self)
    }

    fn accepts(ty: &Type) -> bool {
        *ty == Type::TIMESTAMP
    }
}

/// A `date` as ISO 8601: `2024-01-15`.
pub(super) struct PgDateText(pub(super) String);

impl<'a> FromSql<'a> for PgDateText {
    fn from_sql(_ty: &Type, raw: &'a [u8]) -> DecodeResult<Self> {
        let days: [u8; 4] = raw.try_into().map_err(|_| "a date is 4 bytes")?;
        let days = i64::from(i32::from_be_bytes(days)) + PG_EPOCH_DAYS;
        calendar::date(days).map(Self).ok_or_else(|| "date out of range".into())
    }

    fn accepts(ty: &Type) -> bool {
        *ty == Type::DATE
    }
}
