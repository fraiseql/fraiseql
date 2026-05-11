#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use serde_json::json;

use super::*;

#[test]
fn test_where_operator_from_str() {
    assert_eq!(WhereOperator::from_str("eq").unwrap(), WhereOperator::Eq);
    assert_eq!(WhereOperator::from_str("icontains").unwrap(), WhereOperator::Icontains);
    assert_eq!(WhereOperator::from_str("gte").unwrap(), WhereOperator::Gte);
    assert!(
        matches!(WhereOperator::from_str("unknown"), Err(FraiseQLError::Validation { .. })),
        "expected Validation error for unknown operator"
    );
}

#[test]
fn test_where_operator_expects_array() {
    assert!(WhereOperator::In.expects_array());
    assert!(WhereOperator::Nin.expects_array());
    assert!(!WhereOperator::Eq.expects_array());
}

#[test]
fn test_where_operator_is_case_insensitive() {
    assert!(WhereOperator::Icontains.is_case_insensitive());
    assert!(WhereOperator::Ilike.is_case_insensitive());
    assert!(!WhereOperator::Contains.is_case_insensitive());
}

#[test]
fn test_where_clause_simple() {
    let clause = WhereClause::Field {
        path:     vec!["email".to_string()],
        operator: WhereOperator::Eq,
        value:    json!("test@example.com"),
    };

    assert!(!clause.is_empty());
}

#[test]
fn test_where_clause_and() {
    let clause = WhereClause::And(vec![
        WhereClause::Field {
            path:     vec!["published".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(true),
        },
        WhereClause::Field {
            path:     vec!["views".to_string()],
            operator: WhereOperator::Gte,
            value:    json!(100),
        },
    ]);

    assert!(!clause.is_empty());
}

#[test]
fn test_where_clause_empty() {
    let clause = WhereClause::And(vec![]);
    assert!(clause.is_empty());
}

#[test]
fn test_from_graphql_json_simple_field() {
    let json = json!({ "status": { "eq": "active" } });
    let clause = WhereClause::from_graphql_json(&json).unwrap();
    assert_eq!(
        clause,
        WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("active"),
        }
    );
}

#[test]
fn test_from_graphql_json_camelcase_field_normalized_to_snake_case() {
    let json = json!({ "ipAddress": { "eq": "10.0.0.1" } });
    let clause = WhereClause::from_graphql_json(&json).unwrap();
    assert_eq!(
        clause,
        WhereClause::Field {
            path:     vec!["ip_address".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("10.0.0.1"),
        }
    );
}

#[test]
fn test_from_graphql_json_snake_case_field_unchanged() {
    let json = json!({ "ip_address": { "eq": "10.0.0.1" } });
    let clause = WhereClause::from_graphql_json(&json).unwrap();
    assert_eq!(
        clause,
        WhereClause::Field {
            path:     vec!["ip_address".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("10.0.0.1"),
        }
    );
}

#[test]
fn test_from_graphql_json_multiple_fields() {
    let json = json!({
        "status": { "eq": "active" },
        "age": { "gte": 18 }
    });
    let clause = WhereClause::from_graphql_json(&json).unwrap();
    match clause {
        WhereClause::And(conditions) => assert_eq!(conditions.len(), 2),
        _ => panic!("expected And"),
    }
}

#[test]
fn test_from_graphql_json_logical_combinators() {
    let json = json!({
        "_or": [
            { "role": { "eq": "admin" } },
            { "role": { "eq": "superadmin" } }
        ]
    });
    let clause = WhereClause::from_graphql_json(&json).unwrap();
    match clause {
        WhereClause::Or(conditions) => assert_eq!(conditions.len(), 2),
        _ => panic!("expected Or"),
    }
}

#[test]
fn test_from_graphql_json_not() {
    let json = json!({ "_not": { "deleted": { "eq": true } } });
    let clause = WhereClause::from_graphql_json(&json).unwrap();
    assert!(matches!(clause, WhereClause::Not(_)));
}

#[test]
fn test_from_graphql_json_invalid_operator() {
    let json = json!({ "field": { "nonexistent_op": 42 } });
    let result = WhereClause::from_graphql_json(&json);
    assert!(
        matches!(result, Err(FraiseQLError::Validation { .. })),
        "expected Validation error, got: {result:?}"
    );
}

// ── Nested relation WHERE tests (issue #196) ─────────────────────────────

#[test]
fn test_nested_relation_where_builds_path() {
    let json = json!({ "machine": { "id": { "eq": "abc" } } });
    let clause = WhereClause::from_graphql_json(&json).unwrap();
    assert_eq!(
        clause,
        WhereClause::Field {
            path:     vec!["machine".to_string(), "id".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("abc"),
        }
    );
}

#[test]
fn test_nested_relation_where_camelcase_normalized() {
    let json = json!({ "machineGroup": { "ipAddress": { "eq": "10.0.0.1" } } });
    let clause = WhereClause::from_graphql_json(&json).unwrap();
    assert_eq!(
        clause,
        WhereClause::Field {
            path:     vec!["machine_group".to_string(), "ip_address".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("10.0.0.1"),
        }
    );
}

#[test]
fn test_nested_relation_where_multiple_operators() {
    let json =
        json!({ "machine": { "id": { "eq": "abc" } , "name": { "icontains": "test" } } });
    let clause = WhereClause::from_graphql_json(&json).unwrap();
    // Two nested fields → AND combination
    match clause {
        WhereClause::And(conditions) => {
            assert_eq!(conditions.len(), 2);
            // Both should have path ["machine", ...]
            for cond in &conditions {
                match cond {
                    WhereClause::Field { path, .. } => {
                        assert_eq!(path[0], "machine");
                    },
                    other => panic!("expected Field, got {other:?}"),
                }
            }
        },
        _ => panic!("expected And for multiple nested conditions"),
    }
}

#[test]
fn test_unknown_operator_still_errors() {
    // "bogus" is neither a known operator nor a valid nested field (its value is
    // a plain string, not an object), so the recursion hits the "must be an
    // object" validation.
    let json = json!({ "name": { "bogus": "value" } });
    assert!(WhereClause::from_graphql_json(&json).is_err());
}

#[test]
fn test_new_string_operators_from_str() {
    assert_eq!(WhereOperator::from_str("nlike").unwrap(), WhereOperator::Nlike);
    assert_eq!(WhereOperator::from_str("nilike").unwrap(), WhereOperator::Nilike);
    assert_eq!(WhereOperator::from_str("regex").unwrap(), WhereOperator::Regex);
    assert_eq!(WhereOperator::from_str("iregex").unwrap(), WhereOperator::Iregex);
    assert_eq!(WhereOperator::from_str("nregex").unwrap(), WhereOperator::Nregex);
    assert_eq!(WhereOperator::from_str("niregex").unwrap(), WhereOperator::Niregex);
}

#[test]
fn test_v1_aliases_from_str() {
    // notin → Nin
    assert_eq!(WhereOperator::from_str("notin").unwrap(), WhereOperator::Nin);
    // inrange → InSubnet
    assert_eq!(WhereOperator::from_str("inrange").unwrap(), WhereOperator::InSubnet);
    // imatches → Iregex
    assert_eq!(WhereOperator::from_str("imatches").unwrap(), WhereOperator::Iregex);
    // not_matches → Nregex
    assert_eq!(WhereOperator::from_str("not_matches").unwrap(), WhereOperator::Nregex);
}

#[test]
fn test_new_operators_case_insensitive_flag() {
    assert!(WhereOperator::Nilike.is_case_insensitive());
    assert!(WhereOperator::Iregex.is_case_insensitive());
    assert!(WhereOperator::Niregex.is_case_insensitive());
    assert!(!WhereOperator::Nlike.is_case_insensitive());
    assert!(!WhereOperator::Regex.is_case_insensitive());
    assert!(!WhereOperator::Nregex.is_case_insensitive());
}

#[test]
fn test_nested_relation_filter_builds_multi_segment_path() {
    // where: { machine: { id: { eq: "some-uuid" } } }
    let json = json!({ "machine": { "id": { "eq": "some-uuid" } } });
    let clause = WhereClause::from_graphql_json(&json).unwrap();
    assert_eq!(
        clause,
        WhereClause::Field {
            path:     vec!["machine".to_string(), "id".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("some-uuid"),
        }
    );
}

#[test]
fn test_nested_relation_filter_multiple_fields() {
    // where: { machine: { id: { eq: "uuid" }, name: { contains: "x" } } }
    let json = json!({ "machine": { "id": { "eq": "uuid" }, "name": { "contains": "x" } } });
    let clause = WhereClause::from_graphql_json(&json).unwrap();
    match clause {
        WhereClause::And(conditions) => {
            assert_eq!(conditions.len(), 2);
            assert!(
                conditions.iter().all(|c| matches!(c, WhereClause::Field { .. })),
                "all conditions should be Field with multi-segment paths"
            );
        },
        other => panic!("expected And of Fields, got: {other:?}"),
    }
}

#[test]
fn test_deeply_nested_filter_builds_three_segment_path() {
    // where: { items: { product: { category: { eq: "electronics" } } } }
    let json = json!({ "items": { "product": { "category": { "eq": "electronics" } } } });
    let clause = WhereClause::from_graphql_json(&json).unwrap();
    assert_eq!(
        clause,
        WhereClause::Field {
            path:     vec![
                "items".to_string(),
                "product".to_string(),
                "category".to_string(),
            ],
            operator: WhereOperator::Eq,
            value:    json!("electronics"),
        }
    );
}

#[test]
fn test_unknown_operator_scalar_value_still_errors() {
    // A truly unknown operator with a scalar value should still give the
    // original "Unknown WHERE operator" error, not the nested relation hint.
    let json = json!({ "field": { "nonexistent_op": 42 } });
    let result = WhereClause::from_graphql_json(&json);
    match result {
        Err(FraiseQLError::Validation { message, .. }) => {
            assert!(
                message.contains("Unknown WHERE operator"),
                "expected unknown operator error, got: {message}"
            );
        },
        other => panic!("expected Validation error, got: {other:?}"),
    }
}

#[test]
fn test_new_operators_are_string_operators() {
    assert!(WhereOperator::Nlike.is_string_operator());
    assert!(WhereOperator::Nilike.is_string_operator());
    assert!(WhereOperator::Regex.is_string_operator());
    assert!(WhereOperator::Iregex.is_string_operator());
    assert!(WhereOperator::Nregex.is_string_operator());
    assert!(WhereOperator::Niregex.is_string_operator());
}
