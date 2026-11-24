//! The bound of a time claim: the last whole second a `Timestamp` holds,
//! 9999-12-30T22:00:00Z, is an instant; the next one, and the rest of year 9999 after it,
//! are not.

use super::instant_from_claim;

/// 9999-12-30T22:00:00Z, in seconds from the Unix epoch.
const LAST_SECOND: i64 = 253_402_207_200;
/// 9999-12-31T23:59:59Z.
const END_OF_9999: i64 = 253_402_300_799;

#[test]
fn a_claim_at_the_last_second_an_instant_holds_is_an_instant() {
    let at = instant_from_claim(LAST_SECOND).expect("an instant");
    assert_eq!(at.to_rfc3339(), "9999-12-30T22:00:00+00:00");
}

#[test]
fn a_claim_past_the_last_second_an_instant_holds_is_none() {
    for seconds in [LAST_SECOND + 1, END_OF_9999, END_OF_9999 + 1, i64::MAX] {
        assert_eq!(instant_from_claim(seconds), None, "{seconds}");
    }
}
