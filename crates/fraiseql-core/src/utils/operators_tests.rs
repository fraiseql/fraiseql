//! The registry and the WHERE parser must agree in both directions.
//!
//! #828 was the two of them drifting: the registry advertised 79 names, the
//! parser understood 52, and the 27-name gap turned every `?status[ne]=…` into
//! a 400 *after* validation had accepted it.

use fraiseql_db::WhereOperator;

use super::*;

#[test]
fn every_registry_name_parses() {
    for name in OPERATOR_REGISTRY.keys() {
        assert!(
            WhereOperator::from_str(name).is_ok(),
            "OPERATOR_REGISTRY advertises {name:?}, which the WHERE parser rejects — the REST \
             surface would accept a filter the executor then refuses (#828)"
        );
    }
}

#[test]
fn every_parseable_name_is_advertised() {
    for spec in WHERE_OPERATORS {
        for name in spec.all_names() {
            assert!(
                OPERATOR_REGISTRY.contains_key(name),
                "the WHERE parser accepts {name:?} but OPERATOR_REGISTRY does not advertise it"
            );
        }
    }
}

#[test]
fn the_names_that_used_to_400_are_present_and_parseable() {
    for name in [
        "ne",
        "is_null",
        "is_not_null",
        "contained_in",
        "search",
        "plainto_tsquery",
    ] {
        assert!(is_operator(name), "{name} must be advertised");
        assert!(WhereOperator::from_str(name).is_ok(), "{name} must parse");
    }
}

#[test]
fn the_names_with_no_executor_support_are_no_longer_advertised() {
    // These were in the old hand-maintained registry with no `WhereOperator`
    // variant behind them, so every request that used one was accepted and then
    // rejected. Advertising an operator the executor cannot run is the defect;
    // removing the name is the fix.
    for name in [
        "has_key",
        "has_any_keys",
        "has_all_keys",
        "array_eq",
        "array_neq",
        "notInSubnet",
        "contains_date",
        "adjacent",
        "strictly_left",
        "strictly_right",
        "not_left",
        "not_right",
        "distance_within",
    ] {
        assert!(
            !is_operator(name),
            "{name} has no WhereOperator variant; advertising it produces a 400 after validation \
             has already said yes"
        );
    }
}

#[test]
fn category_lookup_still_works() {
    let comparison = get_operators_by_category(OperatorCategory::Comparison);
    assert!(
        comparison.len() >= 8,
        "expected the comparison family, got {}",
        comparison.len()
    );
    assert_eq!(get_operator_info("eq").expect("eq is registered").sql_op, "=");
    assert!(!is_operator("definitely_not_an_operator"));
}
