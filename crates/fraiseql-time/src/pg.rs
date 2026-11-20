//! A `timestamptz` on the wire: whole microseconds from 2000-01-01T00:00:00Z.

use jiff::SignedDuration;

use crate::{Error, Timestamp};

/// 2000-01-01T00:00:00Z, PostgreSQL's epoch.
const PG_EPOCH: jiff::Timestamp = jiff::Timestamp::constant(946_684_800, 0);

/// The microseconds from [`PG_EPOCH`] to `instant`, truncated toward it.
pub fn to_pg_microseconds(instant: Timestamp) -> i64 {
    let since = instant.0.duration_since(PG_EPOCH);
    // Seconds and the fraction carry the same sign, so this truncates toward the epoch.
    // A `Timestamp` is within ±20,000 years of it, far inside i64 microseconds.
    since.as_secs() * 1_000_000 + i64::from(since.subsec_micros())
}

/// The instant `microseconds` from [`PG_EPOCH`].
///
/// # Errors
///
/// Returns [`Error::OutOfRange`], naming the value, for `infinity`, `-infinity` and
/// any value outside the range a [`Timestamp`] holds.
pub fn from_pg_microseconds(microseconds: i64) -> Result<Timestamp, Error> {
    PG_EPOCH
        .checked_add(SignedDuration::from_micros(microseconds))
        .map(Timestamp)
        .map_err(|_| Error::OutOfRange {
            value: match microseconds {
                i64::MAX => "timestamptz infinity".to_owned(),
                i64::MIN => "timestamptz -infinity".to_owned(),
                other => format!("timestamptz {other} µs from 2000-01-01T00:00:00Z"),
            },
        })
}
