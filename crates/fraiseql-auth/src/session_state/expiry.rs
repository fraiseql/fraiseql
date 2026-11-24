//! An entry's expiry, held to the instants a [`Timestamp`] holds.
//!
//! A TTL is configured in seconds with no upper bound, so `now + ttl` can pass year 9999.
//! Such an entry is kept as long as an instant can say, the last one a `Timestamp` holds,
//! rather than refused or panicking on every write; and a row stored with an expiry past
//! it, when one could be written, reads as that same last instant.

use fraiseql_time::{Timestamp, jiff::SignedDuration};
use sqlx::Row as _;

/// The last instant a [`Timestamp`] holds.
fn last() -> Timestamp {
    fraiseql_time::jiff::Timestamp::MAX.into()
}

/// The expiry of an entry written at `now` that lives `ttl`.
pub(super) fn expiry(now: Timestamp, ttl: SignedDuration) -> Timestamp {
    now.checked_add(ttl).unwrap_or_else(last)
}

/// The stored `expires_at` of `row`: a value past the instants a [`Timestamp`] holds reads
/// as the last one.
pub(super) fn stored_expiry(row: &sqlx::postgres::PgRow) -> Timestamp {
    row.try_get("expires_at").unwrap_or_else(|_| last())
}

#[cfg(test)]
#[path = "expiry_tests.rs"]
mod tests;
