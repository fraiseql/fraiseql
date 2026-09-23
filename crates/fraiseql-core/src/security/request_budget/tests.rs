//! Unit pins for the per-request allowances: that a cost budget accumulates across the
//! reads that share it, and that the two ceilings a request holds do not get crossed.
//!
//! What cannot be pinned here is whether the reads of one request actually *share* one
//! of these — that is a property of the wiring, not of the type, and only a served
//! request can tell a shared budget from a per-read one. It is asserted through the
//! wire in `rest_embedding_read_budget_e2e_pg`.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use super::*;

/// The whole point of the type: two reads that each fit are refused together.
///
/// A `CostBudget` rebuilt per read passes this at the second charge, which is the
/// defect `per_request_max` had while it was scored in `resolve_direct_read` from the
/// configuration rather than charged against a total.
#[test]
fn two_reads_of_one_request_charge_one_running_total() {
    let budget = CostBudget::new(Some(100)).unwrap();

    budget.charge(60).expect("the first read fits");
    assert_eq!(budget.used(), 60);

    let err = budget.charge(60).expect_err("the two together do not");
    match err {
        FraiseQLError::CostExceeded {
            cost,
            limit,
            retry_after_secs,
            ..
        } => {
            assert_eq!(cost, 120, "the figure reported is the total that crossed the line");
            assert_eq!(limit, 100);
            assert_eq!(retry_after_secs, None, "a per-request ceiling is permanent, not a window");
        },
        other => panic!("expected CostExceeded, got {other:?}"),
    }
}

/// The other direction, and the reason it is here: accumulating must not have replaced
/// the single-read refusal. Without this, a `charge` that never compared anything would
/// still pass the test above by way of the second charge alone.
#[test]
fn one_read_over_the_ceiling_is_still_refused_on_its_own() {
    let budget = CostBudget::new(Some(100)).unwrap();
    let err = budget.charge(101).expect_err("one read may exceed the ceiling by itself");
    assert!(
        matches!(
            err,
            FraiseQLError::CostExceeded {
                cost: 101,
                limit: 100,
                ..
            }
        ),
        "{err:?}"
    );
}

/// Exactly the ceiling is served. The refusal is `>`, not `>=`, matching the ceiling
/// `run_gate1` applies to a document so the same number means the same thing whichever
/// way a request arrived.
#[test]
fn a_total_equal_to_the_ceiling_is_served() {
    let budget = CostBudget::new(Some(100)).unwrap();
    budget.charge(40).unwrap();
    budget.charge(60).expect("100 is not over 100");
}

/// "No ceiling declared" stays a shape rather than becoming a very large number.
#[test]
fn no_declared_ceiling_is_no_budget() {
    assert!(CostBudget::new(None).is_none());
}

/// The two allowances are not transposed.
///
/// Both constructor arguments are `Option<u64>`, so swapping them compiles and would
/// leave every deployment enforcing each ceiling as the other — the bytes ceiling
/// refusing on cost and vice versa. Distinct values are the only thing that can tell.
#[test]
fn a_request_holds_its_two_ceilings_unswapped() {
    let budget = RequestBudget::new(Some(4096), Some(100));

    assert_eq!(budget.bytes().expect("a bytes ceiling was declared").limit(), 4096);
    assert_eq!(budget.cost().expect("a cost ceiling was declared").limit(), 100);
}

/// Each allowance is absent independently: an operator writes `[validation]
/// max_response_bytes` and `[security.cost_budget] per_request_max` in different
/// sections, and declaring one must not conjure the other.
#[test]
fn either_ceiling_may_be_absent_on_its_own() {
    let bytes_only = RequestBudget::new(Some(4096), None);
    assert!(bytes_only.bytes().is_some());
    assert!(bytes_only.cost().is_none(), "no cost ceiling was declared");

    let cost_only = RequestBudget::new(None, Some(100));
    assert!(cost_only.bytes().is_none(), "no bytes ceiling was declared");
    assert!(cost_only.cost().is_some());

    let neither = RequestBudget::new(None, None);
    assert!(neither.bytes().is_none());
    assert!(neither.cost().is_none());
}
