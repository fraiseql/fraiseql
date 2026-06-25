//! Tests for the EXPLAIN SQL builders, co-located with `support/explain.rs`.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use super::*;

/// #486: the EXPLAIN WHERE clause must key on the `snake_case` JSONB column,
/// not the raw camelCase variable name — otherwise EXPLAIN runs against a
/// never-matching `data->>'organizationId'` while the real query uses
/// `data->>'organization_id'`. This is the sixth recasing surface (the
/// consolidated parity canary in `query_params_tests.rs` cannot reach this
/// private function).
#[test]
fn explain_recases_where_variables_to_snake() {
    let vars = serde_json::json!({ "organizationId": "x" });
    let clause = build_where_from_variables(Some(&vars)).expect("clause built");
    match clause {
        WhereClause::Field { path, .. } => {
            assert_eq!(path, vec!["organization_id".to_string()]);
        },
        other => panic!("expected WhereClause::Field, got {other:?}"),
    }
}

#[test]
fn explain_display_sql_recases_keys_to_snake() {
    // The human-readable echo must match the column the executed EXPLAIN
    // actually filters on (consistent with `build_where_from_variables`).
    let vars = serde_json::json!({ "organizationId": "x" });
    let sql = build_display_sql("v_orders", Some(&vars), None, None);
    assert!(sql.contains("data->>'organization_id'"), "got: {sql}");
    assert!(!sql.contains("organizationId"), "must not echo the camel key: {sql}");
}

#[test]
fn explain_display_sql_single_word_unchanged() {
    let vars = serde_json::json!({ "status": "open" });
    let sql = build_display_sql("v_orders", Some(&vars), None, None);
    assert!(sql.contains("data->>'status'"), "got: {sql}");
}
