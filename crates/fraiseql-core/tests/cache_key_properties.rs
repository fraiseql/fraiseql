//! Property-based tests for cache key generation.
//!
//! Verifies security-critical and correctness invariants:
//!
//! 1. **Determinism** — same inputs always produce the same key.
//! 2. **Key uniqueness** — different variables produce different keys (security).
//! 3. **Schema isolation** — different schema versions produce different keys.
//! 4. **Output format** — keys are always 64-char lowercase hex (SHA-256).
//! 5. **WHERE isolation** — different WHERE clauses produce different keys.

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use fraiseql_core::{
    cache::generate_cache_key,
    db::{WhereClause, WhereOperator},
};
use proptest::prelude::*;
use serde_json::json;

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

/// Generate an arbitrary JSON value suitable for GraphQL variables.
fn arb_variables() -> impl Strategy<Value = serde_json::Value> {
    prop_oneof![
        Just(json!({})),
        Just(json!(null)),
        any::<i64>().prop_map(|n| json!({"n": n})),
        "[a-z]{1,20}".prop_map(|s| json!({"s": s})),
        (any::<i64>(), "[a-z]{1,10}").prop_map(|(n, s)| json!({"n": n, "s": s})),
        prop::collection::vec("[a-z]{1,5}", 1..5).prop_map(|v| json!({"tags": v})),
    ]
}

/// Generate a simple query string.
fn arb_query() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("query { users { id } }".to_string()),
        Just("query { user(id: $id) { name } }".to_string()),
        Just("query { posts(limit: $limit) { title } }".to_string()),
        "[a-z]{3,10}".prop_map(|name| format!("query {{ {name} {{ id }} }}")),
    ]
}

/// Generate a schema version string.
fn arb_schema_version() -> impl Strategy<Value = String> {
    "[a-f0-9]{8,16}"
}

// ---------------------------------------------------------------------------
// Property: Determinism — same inputs → same key
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn cache_key_is_deterministic(
        query in arb_query(),
        vars in arb_variables(),
        version in arb_schema_version(),
    ) {
        let key1 = generate_cache_key(&query, &vars, None, &version);
        let key2 = generate_cache_key(&query, &vars, None, &version);
        prop_assert_eq!(key1, key2, "Cache key must be deterministic");
    }
}

// ---------------------------------------------------------------------------
// Property: Output format — always 64-char lowercase hex
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn cache_key_is_valid_sha256_hex(
        query in arb_query(),
        vars in arb_variables(),
        version in arb_schema_version(),
    ) {
        let key = generate_cache_key(&query, &vars, None, &version);
        prop_assert_eq!(key.len(), 64, "SHA-256 hex must be 64 chars");
        prop_assert!(
            key.chars().all(|c| c.is_ascii_hexdigit()),
            "Key must be hexadecimal: {key}"
        );
    }
}

// ---------------------------------------------------------------------------
// Property: Different variables → different keys (SECURITY CRITICAL)
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn different_string_variables_produce_different_keys(
        a in "[a-z]{1,20}",
        b in "[a-z]{1,20}",
    ) {
        prop_assume!(a != b);
        let query = "query getUser($id: ID!) { user(id: $id) { name } }";
        let key_a = generate_cache_key(query, &json!({"id": a}), None, "v1");
        let key_b = generate_cache_key(query, &json!({"id": b}), None, "v1");
        prop_assert_ne!(
            key_a, key_b,
            "SECURITY: different variable values must produce different keys"
        );
    }

    #[test]
    fn different_numeric_variables_produce_different_keys(
        a in any::<i64>(),
        b in any::<i64>(),
    ) {
        prop_assume!(a != b);
        let query = "query getUsers($limit: Int!) { users(limit: $limit) { id } }";
        let key_a = generate_cache_key(query, &json!({"limit": a}), None, "v1");
        let key_b = generate_cache_key(query, &json!({"limit": b}), None, "v1");
        prop_assert_ne!(
            key_a, key_b,
            "SECURITY: different numeric variables must produce different keys"
        );
    }
}

// ---------------------------------------------------------------------------
// Property: Different schema versions → different keys
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn different_schema_versions_produce_different_keys(
        query in arb_query(),
        vars in arb_variables(),
        v1 in arb_schema_version(),
        v2 in arb_schema_version(),
    ) {
        prop_assume!(v1 != v2);
        let key1 = generate_cache_key(&query, &vars, None, &v1);
        let key2 = generate_cache_key(&query, &vars, None, &v2);
        prop_assert_ne!(
            key1, key2,
            "Different schema versions must produce different keys"
        );
    }
}

// ---------------------------------------------------------------------------
// Property: Different WHERE clauses → different keys
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn different_where_values_produce_different_keys(
        a in "[a-z]{1,20}",
        b in "[a-z]{1,20}",
    ) {
        prop_assume!(a != b);
        let query = "query { users { id } }";

        let where_a = WhereClause::Field {
            path: vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value: json!(a),
        };
        let where_b = WhereClause::Field {
            path: vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value: json!(b),
        };

        let key_a = generate_cache_key(query, &json!({}), Some(&where_a), "v1");
        let key_b = generate_cache_key(query, &json!({}), Some(&where_b), "v1");
        prop_assert_ne!(
            key_a, key_b,
            "Different WHERE clause values must produce different keys"
        );
    }

    #[test]
    fn different_where_operators_produce_different_keys(
        field in "[a-z]{1,10}",
    ) {
        let query = "query { users { id } }";
        let value = json!(42);

        let where_eq = WhereClause::Field {
            path: vec![field.clone()],
            operator: WhereOperator::Eq,
            value: value.clone(),
        };
        let where_gt = WhereClause::Field {
            path: vec![field],
            operator: WhereOperator::Gt,
            value,
        };

        let key_eq = generate_cache_key(query, &json!({}), Some(&where_eq), "v1");
        let key_gt = generate_cache_key(query, &json!({}), Some(&where_gt), "v1");
        prop_assert_ne!(
            key_eq, key_gt,
            "Different WHERE operators must produce different keys"
        );
    }
}

// ---------------------------------------------------------------------------
// Property: With vs without WHERE clause → different keys
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn presence_of_where_clause_changes_key(
        query in arb_query(),
        field in "[a-z]{1,10}",
        value in "[a-z]{1,20}",
    ) {
        let where_clause = WhereClause::Field {
            path: vec![field],
            operator: WhereOperator::Eq,
            value: json!(value),
        };

        let key_without = generate_cache_key(&query, &json!({}), None, "v1");
        let key_with = generate_cache_key(&query, &json!({}), Some(&where_clause), "v1");
        prop_assert_ne!(
            key_without, key_with,
            "Presence of WHERE clause must change key"
        );
    }
}
