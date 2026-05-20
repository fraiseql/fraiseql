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
        path: vec!["email".to_string()],
        operator: WhereOperator::Eq,
        value: json!("test@example.com"),
    };

    assert!(!clause.is_empty());
}

#[test]
fn test_where_clause_and() {
    let clause = WhereClause::And(vec![
        WhereClause::Field {
            path: vec!["published".to_string()],
            operator: WhereOperator::Eq,
            value: json!(true),
        },
        WhereClause::Field {
            path: vec!["views".to_string()],
            operator: WhereOperator::Gte,
            value: json!(100),
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
            path: vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value: json!("active"),
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
            path: vec!["ip_address".to_string()],
            operator: WhereOperator::Eq,
            value: json!("10.0.0.1"),
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
            path: vec!["ip_address".to_string()],
            operator: WhereOperator::Eq,
            value: json!("10.0.0.1"),
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
            path: vec!["machine".to_string(), "id".to_string()],
            operator: WhereOperator::Eq,
            value: json!("abc"),
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
            path: vec!["machine_group".to_string(), "ip_address".to_string()],
            operator: WhereOperator::Eq,
            value: json!("10.0.0.1"),
        }
    );
}

#[test]
fn test_nested_relation_where_multiple_operators() {
    let json = json!({ "machine": { "id": { "eq": "abc" } , "name": { "icontains": "test" } } });
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
            path: vec!["machine".to_string(), "id".to_string()],
            operator: WhereOperator::Eq,
            value: json!("some-uuid"),
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
            path: vec![
                "items".to_string(),
                "product".to_string(),
                "category".to_string(),
            ],
            operator: WhereOperator::Eq,
            value: json!("electronics"),
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

// ── Cycle 1: to_snake_case works for operator names ─────────────────────

#[test]
fn test_to_snake_case_for_operator_names() {
    use crate::utils::to_snake_case;

    assert_eq!(to_snake_case("descendantOfId"), "descendant_of_id");
    assert_eq!(to_snake_case("ancestorOfId"), "ancestor_of_id");
    assert_eq!(to_snake_case("isPrivate"), "is_private");
    assert_eq!(to_snake_case("inSubnet"), "in_subnet");
    assert_eq!(to_snake_case("already_snake"), "already_snake");
    assert_eq!(to_snake_case("simple"), "simple");
}

// ── Cycle 2: Smart normalization in from_str ────────────────────────────

#[test]
fn test_operator_normalization_camel_to_registered_snake() {
    // descendantOf → descendant_of (registered)
    let op = WhereOperator::from_str("descendantOf");
    assert!(op.is_ok(), "descendantOf should normalize to descendant_of");
    assert_eq!(op.unwrap(), WhereOperator::DescendantOf);
}

#[test]
fn test_operator_normalization_ancestor_of() {
    let op = WhereOperator::from_str("ancestorOf");
    assert!(op.is_ok(), "ancestorOf should normalize to ancestor_of");
    assert_eq!(op.unwrap(), WhereOperator::AncestorOf);
}

#[test]
fn test_operator_normalization_preserves_registered_camel() {
    // inSubnet is registered as "in_subnet" | "inrange", not as "inSubnet"
    // But we don't have a camelCase-registered operator currently.
    // The key behavior: if the camelCase form is not registered, convert to
    // snake_case and try again.
    let op = WhereOperator::from_str("inSubnet");
    assert!(op.is_ok(), "inSubnet should normalize to in_subnet");
    assert_eq!(op.unwrap(), WhereOperator::InSubnet);
}

#[test]
fn test_operator_normalization_rejects_unknown() {
    let op = WhereOperator::from_str("totallyBogusOp");
    assert!(op.is_err(), "unknown camelCase operator should be rejected");
}

#[test]
fn test_operator_normalization_passthrough_snake_case() {
    // Already snake_case, no conversion attempted
    let op = WhereOperator::from_str("descendant_of");
    assert!(op.is_ok());
    assert_eq!(op.unwrap(), WhereOperator::DescendantOf);
}

#[test]
fn test_operator_normalization_hierarchy_operators() {
    // All hierarchy operators should resolve from camelCase
    assert_eq!(WhereOperator::from_str("matchesLquery").unwrap(), WhereOperator::MatchesLquery);
    assert_eq!(
        WhereOperator::from_str("matchesLtxtquery").unwrap(),
        WhereOperator::MatchesLtxtquery
    );
    assert_eq!(
        WhereOperator::from_str("matchesAnyLquery").unwrap(),
        WhereOperator::MatchesAnyLquery
    );
}

#[test]
fn test_operator_normalization_network_operators() {
    assert_eq!(WhereOperator::from_str("isPrivate").unwrap(), WhereOperator::IsPrivate);
    assert_eq!(WhereOperator::from_str("isLoopback").unwrap(), WhereOperator::IsLoopback);
    assert_eq!(WhereOperator::from_str("isMulticast").unwrap(), WhereOperator::IsMulticast);
    assert_eq!(WhereOperator::from_str("isLinkLocal").unwrap(), WhereOperator::IsLinkLocal);
    assert_eq!(
        WhereOperator::from_str("isDocumentation").unwrap(),
        WhereOperator::IsDocumentation
    );
    assert_eq!(
        WhereOperator::from_str("isCarrierGrade").unwrap(),
        WhereOperator::IsCarrierGrade
    );
    assert_eq!(
        WhereOperator::from_str("containsSubnet").unwrap(),
        WhereOperator::ContainsSubnet
    );
    assert_eq!(WhereOperator::from_str("containsIp").unwrap(), WhereOperator::ContainsIP);
    assert_eq!(
        WhereOperator::from_str("strictlyContains").unwrap(),
        WhereOperator::StrictlyContains
    );
}

// ── Cycle 3: Integration with WHERE clause parsing ──────────────────────

#[test]
fn test_where_clause_with_camel_case_operator() {
    let json = json!({
        "ip_address": { "descendantOf": 42 }
    });
    let clause = WhereClause::from_graphql_json(&json).unwrap();
    assert_eq!(
        clause,
        WhereClause::Field {
            path: vec!["ip_address".to_string()],
            operator: WhereOperator::DescendantOf,
            value: json!(42),
        }
    );
}

#[test]
fn test_where_clause_with_camel_case_network_operator() {
    let json = json!({
        "ip_address": { "isPrivate": true }
    });
    let clause = WhereClause::from_graphql_json(&json).unwrap();
    assert_eq!(
        clause,
        WhereClause::Field {
            path: vec!["ip_address".to_string()],
            operator: WhereOperator::IsPrivate,
            value: json!(true),
        }
    );
}

// --- Issue #250: ID-based ltree operators ---

#[test]
fn test_descendant_of_id_from_str() {
    assert_eq!(
        WhereOperator::from_str("descendant_of_id").unwrap(),
        WhereOperator::DescendantOfId
    );
}

#[test]
fn test_ancestor_of_id_from_str() {
    assert_eq!(WhereOperator::from_str("ancestor_of_id").unwrap(), WhereOperator::AncestorOfId);
}

#[test]
fn test_descendant_of_id_camel_case() {
    assert_eq!(
        WhereOperator::from_str("descendantOfId").unwrap(),
        WhereOperator::DescendantOfId
    );
}

#[test]
fn test_ancestor_of_id_camel_case() {
    assert_eq!(WhereOperator::from_str("ancestorOfId").unwrap(), WhereOperator::AncestorOfId);
}

#[test]
fn test_descendant_of_id_graphql_json() {
    let json = json!({
        "category_path": { "descendantOfId": "abc-123" }
    });
    let clause = WhereClause::from_graphql_json(&json).unwrap();
    assert_eq!(
        clause,
        WhereClause::Field {
            path: vec!["category_path".to_string()],
            operator: WhereOperator::DescendantOfId,
            value: json!("abc-123"),
        }
    );
}
