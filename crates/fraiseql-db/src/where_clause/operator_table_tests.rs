//! The operator table and the WHERE parser agree in both directions.

use super::*;
/// The table and the parser agree in both directions.
///
/// This is the invariant #828 violated: a name the surface advertises must
/// be a name the executor can run, and an operator the executor can run
/// must be reachable by name.
#[test]
fn every_advertised_name_parses_to_its_own_variant() {
    for spec in WHERE_OPERATORS {
        for name in spec.all_names() {
            let parsed = WhereOperator::from_str(name);
            assert!(parsed.is_ok(), "advertised operator {name:?} does not parse: {parsed:?}");
            let canonical = WhereOperator::from_str(spec.name);
            assert!(canonical.is_ok(), "canonical {:?} does not parse: {canonical:?}", spec.name);
            assert_eq!(
                parsed.ok(),
                canonical.ok(),
                "alias {name:?} resolves to a different operator than {:?}",
                spec.name
            );
        }
    }
}

#[test]
fn operator_names_are_unique_across_the_table() {
    let mut seen = std::collections::HashSet::new();
    for spec in WHERE_OPERATORS {
        for name in spec.all_names() {
            assert!(seen.insert(name), "operator name {name:?} is declared twice");
        }
    }
}

#[test]
fn requires_array_matches_the_operator_semantics() {
    for spec in WHERE_OPERATORS {
        let op = WhereOperator::from_str(spec.name).expect("canonical name parses");
        if matches!(op, WhereOperator::In | WhereOperator::Nin) {
            assert!(spec.requires_array, "{:?} must require an array", spec.name);
        }
    }
}

#[test]
fn the_rest_bracket_names_that_used_to_400_now_parse() {
    // The exact names #828 reported: accepted by the REST validator and
    // then rejected by the WHERE parser.
    for name in ["ne", "is_null", "is_not_null"] {
        assert!(
            WhereOperator::from_str(name).is_ok(),
            "{name} is advertised by the REST bracket syntax and must parse"
        );
    }
}
