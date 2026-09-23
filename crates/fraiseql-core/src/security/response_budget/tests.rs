//! Unit pins for the response-bytes ceiling: the byte estimator the transport
//! tests depend on, and the budget that accumulates across a streamed read.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use super::*;

/// The arithmetic the executor tests depend on, stated once here so a change to
/// the estimator shows up as a failure in this file rather than as a puzzling
/// off-by-N in a transport test.
#[test]
fn a_flat_document_is_charged_its_keys_values_and_punctuation() {
    let v = serde_json::json!({"id": "1", "name": "Alice"});
    // {} = 2; "id" member = 4 + 2 + (2 + 1) = 9; "name" member = 4 + 4 + (2 + 5) = 15.
    assert_eq!(json_bytes(&v), 26);
}

#[test]
fn an_empty_container_is_charged_only_its_brackets() {
    assert_eq!(json_bytes(&serde_json::json!({})), 2);
    assert_eq!(json_bytes(&serde_json::json!([])), 2);
}

/// Nesting is charged, which is the property that makes this a size ceiling
/// rather than a row count: one row of a deeply nested document is not one unit
/// of work.
#[test]
fn nesting_is_charged_all_the_way_down() {
    let flat = serde_json::json!({"a": "x"});
    let nested = serde_json::json!({"a": {"b": {"c": "x"}}});
    assert!(
        json_bytes(&nested) > json_bytes(&flat),
        "a nested document must cost more than a flat one"
    );
}

#[test]
fn a_column_row_is_charged_its_payload() {
    assert_eq!(column_bytes(&ColumnValue::Text("Alice".into())), 5);
    assert_eq!(column_bytes(&ColumnValue::Null), 0);
    assert_eq!(column_bytes(&ColumnValue::Boolean(true)), 1);
}

/// No ceiling declared means no budget at all — not a budget of `u64::MAX`,
/// which would behave like one right up until it overflowed.
#[test]
fn no_declared_ceiling_yields_no_budget() {
    assert!(ResponseBudget::new(None).is_none());
}

/// The total accumulates, and the refusal reports the running total rather than
/// the charge that crossed the line.
#[test]
fn charges_accumulate_and_the_error_carries_the_running_total() {
    let budget = ResponseBudget::new(Some(10)).expect("a ceiling was declared");
    budget.charge(4).expect("under");
    budget.charge(4).expect("still under");
    assert_eq!(budget.used(), 8);

    let err = budget.charge(4).unwrap_err();
    match err {
        FraiseQLError::ResponseTooLarge { bytes, limit } => {
            assert_eq!(bytes, 12, "4 + 4 + 4, not the 4 that crossed it");
            assert_eq!(limit, 10);
        },
        other => panic!("expected ResponseTooLarge, got {other:?}"),
    }
}

/// **Two holders of one budget charge one total.**
///
/// The contract the whole aggregate bound rests on: a transport that answers one request
/// with several reads hands each of them a `&ResponseBudget`, and the ceiling then bounds
/// the response rather than each read. Charging through `&self` is what makes that
/// possible, so it is asserted here rather than only through a served request — a change
/// back to `&mut self` over a plain `u64` would give every holder its own copy, and the
/// only other test that could notice needs a database.
#[test]
fn two_shared_handles_charge_one_running_total() {
    let budget = ResponseBudget::new(Some(10)).expect("a ceiling was declared");
    let first: &ResponseBudget = &budget;
    let second: &ResponseBudget = &budget;

    first.charge(6).expect("under on its own");
    // Six again would fit a budget of ten if this handle had one of its own.
    let err = second.charge(6).unwrap_err();
    match err {
        FraiseQLError::ResponseTooLarge { bytes, limit } => {
            assert_eq!(bytes, 12, "both handles charged the same total");
            assert_eq!(limit, 10);
        },
        other => panic!("expected ResponseTooLarge, got {other:?}"),
    }
    assert_eq!(budget.used(), 12, "and the total is visible through the owner");
}

/// Exactly at the ceiling is under it — the comparison is `>`, not `>=`, so a
/// deployment that sizes the ceiling to its largest legitimate response still
/// serves that response.
#[test]
fn exactly_at_the_ceiling_is_allowed() {
    let budget = ResponseBudget::new(Some(10)).expect("a ceiling was declared");
    budget.charge(10).expect("10 is not over 10");
}

/// A pathological document cannot wrap the counter into a pass.
#[test]
fn an_overflowing_charge_saturates_rather_than_wrapping() {
    let budget = ResponseBudget::new(Some(u64::MAX - 1)).expect("declared");
    budget.charge(u64::MAX).expect_err("saturates to MAX, which is over MAX - 1");
}
