//! SAML's instants as [`Timestamp`]s, held to the instants one holds (to
//! 9999-12-30T22:00:00Z).
//!
//! Past that bound a certificate's `NotAfter` reads as no expiry: RFC 5280 §4.1.2.5 gives
//! `99991231235959Z` to a certificate with no well-defined expiry, so there is nothing to
//! warn about. An assertion's `NotOnOrAfter` past it is refused: an assertion that never
//! expires is not accepted, as a token that never expires is not.

use fraiseql_time::{Timestamp, jiff::SignedDuration};
use sqlx::Row as _;

use super::SamlError;

/// A certificate's `NotAfter`, `days` and `secs` from now as openssl's diff gives it.
///
/// `None` past the last instant a [`Timestamp`] holds: no expiry.
pub(super) fn certificate_not_after(days: i32, secs: i32) -> Option<Timestamp> {
    Timestamp::now()
        .checked_add(SignedDuration::from_hours(24 * i64::from(days)))?
        .checked_add(SignedDuration::from_secs(i64::from(secs)))
}

/// The stored `certificate_expires_at` of `row`. A value past the last instant a
/// [`Timestamp`] holds, stored when an expiry could reach further, reads as no expiry,
/// as [`certificate_not_after`] reads it.
pub(super) fn stored_certificate_expiry(row: &sqlx::postgres::PgRow) -> Option<Timestamp> {
    row.try_get("certificate_expires_at").ok().flatten()
}

/// An assertion's `NotOnOrAfter`, given as seconds from the Unix epoch and the nanosecond
/// within that second.
///
/// # Errors
///
/// [`SamlError::Malformed`] past the last instant a [`Timestamp`] holds.
pub(super) fn not_on_or_after(seconds: i64, nanosecond: u32) -> Result<Timestamp, SamlError> {
    i32::try_from(nanosecond)
        .ok()
        .and_then(|nanosecond| Timestamp::new(seconds, nanosecond).ok())
        .ok_or_else(|| {
            SamlError::Malformed(
                "assertion NotOnOrAfter is past the last instant this server holds".to_string(),
            )
        })
}

/// The instant `days` after `now` within which a certificate's expiry is warned of: the
/// last instant a [`Timestamp`] holds when further, so every expiry is within it.
pub(super) fn warning_horizon(now: Timestamp, days: i64) -> Timestamp {
    i32::try_from(days)
        .ok()
        .and_then(|days| SignedDuration::from_hours(24).checked_mul(days))
        .and_then(|span| now.checked_add(span))
        .unwrap_or_else(|| fraiseql_time::jiff::Timestamp::MAX.into())
}

#[cfg(test)]
#[path = "time_tests.rs"]
mod tests;
