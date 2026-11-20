//! Conversions between [`Timestamp`] and chrono's `DateTime<Utc>`, for code that has not
//! moved off chrono yet.

use chrono::{DateTime, Utc};

use crate::{Error, Timestamp};

const NANOS_PER_SECOND: i32 = 1_000_000_000;

impl From<Timestamp> for DateTime<Utc> {
    fn from(instant: Timestamp) -> Self {
        // chrono wants a fraction in [0, 1 s); a `Timestamp` before 1970 carries a negative one.
        let (second, nanosecond) = match instant.subsec_nanosecond() {
            n if n < 0 => (instant.as_second() - 1, n + NANOS_PER_SECOND),
            n => (instant.as_second(), n),
        };
        Self::from_timestamp(second, nanosecond.unsigned_abs())
            .expect("a Timestamp lies within years -9999 to 9999, inside chrono's range")
    }
}

impl TryFrom<DateTime<Utc>> for Timestamp {
    type Error = Error;

    /// # Errors
    ///
    /// Returns [`Error::OutOfRange`], naming the value, for an instant outside years -9999
    /// to 9999, which chrono holds and a [`Timestamp`] does not.
    fn try_from(instant: DateTime<Utc>) -> Result<Self, Error> {
        let nanosecond =
            i32::try_from(instant.timestamp_subsec_nanos()).map_err(|_| out_of_range(instant))?;
        Self::new(instant.timestamp(), nanosecond).map_err(|_| out_of_range(instant))
    }
}

fn out_of_range(instant: DateTime<Utc>) -> Error {
    Error::OutOfRange {
        value: instant.to_rfc3339(),
    }
}
