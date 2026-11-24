//! A token's time claim as an instant.
//!
//! Every path that turns a bearer's `exp` into the expiry of a principal reads it here, so
//! the rule is one rule: a claim is whole seconds from the Unix epoch, and one past the
//! instants a [`Timestamp`] holds is refused, not saturated. An `exp` that far out is a
//! token that never expires, and a token that never expires is not accepted.

use fraiseql_time::Timestamp;

/// The instant a JWT time claim (`exp`, `nbf`, `iat`) names, in whole seconds from the
/// Unix epoch.
///
/// `None` when the claim is outside the instants a [`Timestamp`] holds: after
/// 9999-12-30T22:00:00Z, the last whole second jiff's range reaches (it keeps every instant
/// writable under any UTC offset up to 26 hours), or before -9999-01-02T01:59:59Z. The
/// caller refuses the token.
#[must_use]
pub fn instant_from_claim(seconds: i64) -> Option<Timestamp> {
    Timestamp::from_second(seconds).ok()
}

#[cfg(test)]
#[path = "claim_time_tests.rs"]
mod tests;
