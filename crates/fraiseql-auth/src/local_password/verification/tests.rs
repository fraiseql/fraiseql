//! Unit tests for the promotion gate (no database).
//!
//! [`decide_promotion`] is the security-relevant decision of the whole flow, so its
//! truth table is exhaustive here rather than sampled through the database path: the
//! integration suite proves the SQL asks the gate the right question, and this proves
//! the gate answers it correctly.

use super::*;

const ADDR: &str = "user@example.com";
const OTHER_ADDR: &str = "someone-else@example.com";

#[test]
fn unclaimed_address_on_an_unverified_account_promotes() {
    assert_eq!(decide_promotion(None, ADDR, false), PromotionDecision::Promote);
}

#[test]
fn confirming_the_same_address_twice_is_a_no_op_success() {
    assert_eq!(decide_promotion(Some(ADDR), ADDR, false), PromotionDecision::AlreadyVerified);
}

#[test]
fn an_address_another_account_holds_is_refused() {
    // The takeover refusal. Promoting here would put two accounts in one linking key
    // slot, and the only resolution — a merge — would carry this account's password
    // credential onto the other account.
    assert_eq!(
        decide_promotion(None, ADDR, true),
        PromotionDecision::RefuseClaimedByAnotherAccount
    );
}

#[test]
fn the_claimed_refusal_wins_over_every_account_state() {
    // Checked first, unconditionally: whatever this account already carries, an address
    // another account holds is never written.
    for account_email in [None, Some(ADDR), Some(OTHER_ADDR)] {
        assert_eq!(
            decide_promotion(account_email, ADDR, true),
            PromotionDecision::RefuseClaimedByAnotherAccount,
            "claimed address must refuse with account_email = {account_email:?}"
        );
    }
}

#[test]
fn an_account_with_a_different_verified_address_is_not_rekeyed() {
    // A verified address is the account's cross-provider linking key. Re-keying it here
    // would silently move the account out of the key space a previous trusted sign-in
    // placed it in.
    assert_eq!(
        decide_promotion(Some(OTHER_ADDR), ADDR, false),
        PromotionDecision::RefuseAccountHasDifferentEmail
    );
}

#[test]
fn only_promote_and_already_verified_are_writeable_outcomes() {
    // Guards the match in `confirm_email_verification`: every outcome that is not one of
    // these two must refuse, so a future variant cannot default into writing an address.
    let writeable = [
        decide_promotion(None, ADDR, false),
        decide_promotion(Some(ADDR), ADDR, false),
    ];
    for decision in writeable {
        assert!(
            matches!(decision, PromotionDecision::Promote | PromotionDecision::AlreadyVerified),
            "{decision:?} must be a writeable outcome"
        );
    }
    let refusals = [
        decide_promotion(None, ADDR, true),
        decide_promotion(Some(OTHER_ADDR), ADDR, false),
    ];
    for decision in refusals {
        assert!(
            matches!(
                decision,
                PromotionDecision::RefuseClaimedByAnotherAccount
                    | PromotionDecision::RefuseAccountHasDifferentEmail
            ),
            "{decision:?} must be a refusal"
        );
    }
}
