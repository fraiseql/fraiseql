//! Unit tests for query parameter helpers.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use super::*;

#[test]
fn enforce_max_page_size_allows_value_at_or_under_max() {
    assert_eq!(enforce_max_page_size(Some(1000), Some(1000), "limit").unwrap(), Some(1000));
    assert_eq!(enforce_max_page_size(Some(50), Some(1000), "first").unwrap(), Some(50));
}

#[test]
fn enforce_max_page_size_passes_through_when_no_max_configured() {
    // No ceiling → any value is allowed (opt-out).
    assert_eq!(enforce_max_page_size(Some(u32::MAX), None, "limit").unwrap(), Some(u32::MAX));
    // No value supplied → nothing to check.
    assert_eq!(enforce_max_page_size(None, Some(1000), "limit").unwrap(), None);
}

#[test]
fn enforce_max_page_size_rejects_value_over_max() {
    let err = enforce_max_page_size(Some(5_000_000), Some(1000), "first").unwrap_err();
    match err {
        crate::FraiseQLError::Validation { message, path } => {
            assert!(message.contains("first"), "message was: {message}");
            assert!(message.contains("5000000"), "message was: {message}");
            assert!(message.contains("1000"), "message was: {message}");
            assert_eq!(path.as_deref(), Some("first"));
        },
        other => panic!("expected Validation error, got {other:?}"),
    }
}

// ── #486: query-path camelCase → snake_case recasing ─────────────────────────

use std::collections::HashMap;

use crate::schema::{ArgumentDefinition, FieldType};

/// Build a one-argument explicit-arg WHERE clause and return the emitted JSONB path.
fn explicit_arg_path(arg_name: &str) -> Vec<String> {
    let args = [ArgumentDefinition::new(arg_name, FieldType::String)];
    let mut provided = HashMap::new();
    provided.insert(arg_name.to_string(), serde_json::json!("x"));
    let native = HashMap::new();
    let clause = combine_explicit_arg_where(None, &args, &provided, &native)
        .expect("single explicit arg yields a clause");
    match clause {
        WhereClause::Field { path, .. } => path,
        other => panic!("expected WhereClause::Field, got {other:?}"),
    }
}

#[test]
fn explicit_arg_multiword_camel_is_recased_to_snake() {
    // Multi-word camelCase args MUST become snake_case JSONB keys so the filter
    // matches the stored `data->>'organization_id'` instead of always-NULL
    // `data->>'organizationId'` (#486 silent-empty). Mirrors the #456 mutation fix.
    assert_eq!(explicit_arg_path("organizationId"), vec!["organization_id".to_string()]);
    assert_eq!(explicit_arg_path("machineStatus"), vec!["machine_status".to_string()]);
    // Acronym/digit parity with the canonical caser (`dns1Id` → `dns_1_id`).
    assert_eq!(explicit_arg_path("dns1Id"), vec!["dns_1_id".to_string()]);
}

#[test]
fn explicit_arg_single_word_is_unchanged() {
    // Single-token args must not be mangled by over-eager recasing.
    assert_eq!(explicit_arg_path("status"), vec!["status".to_string()]);
    assert_eq!(explicit_arg_path("url"), vec!["url".to_string()]);
}

#[test]
fn inject_param_recasing_is_idempotent_on_snake() {
    // Inject params arrive snake_case from config; recasing must be a no-op so the
    // class is closed once without disturbing the common case.
    let native = HashMap::new();
    let clause = inject_param_where_clause("tenant_id", serde_json::json!("t"), &native);
    match clause {
        WhereClause::Field { path, .. } => assert_eq!(path, vec!["tenant_id".to_string()]),
        other => panic!("expected WhereClause::Field, got {other:?}"),
    }
}

/// Extract the single JSONB field path from a `WhereClause`, unwrapping a
/// one-element `And`. Panics on any other shape.
fn single_field_path(clause: &WhereClause) -> Vec<String> {
    match clause {
        WhereClause::Field { path, .. } => path.clone(),
        WhereClause::And(inner) if inner.len() == 1 => single_field_path(&inner[0]),
        other => panic!("expected a single Field clause, got {other:?}"),
    }
}

/// **Six-surface parity canary (#486).** For one multi-word camel name, every
/// arg-shaped filter surface must resolve to the *identical* snake column. A
/// future filter surface that forgets to recase breaks this test.
///
/// Five surfaces are exercised here (all reachable from this module); the sixth,
/// `explain` (`build_where_from_variables` / `build_display_sql`), is a private
/// EXPLAIN-only path verified by its own sibling test in
/// `executor/support/explain.rs` — see `explain_recases_*`.
#[test]
fn camel_filter_recasing_is_identical_across_surfaces() {
    // `organizationId` → `organization_id` must hold on every arg-shaped surface.
    const SNAKE: &str = "organization_id";

    // 1. Explicit query argument.
    assert_eq!(
        explicit_arg_path("organizationId"),
        vec![SNAKE.to_string()],
        "explicit-arg surface"
    );

    // 2. WHERE-input object (the already-correct reference path).
    let where_input = WhereClause::from_graphql_json(&serde_json::json!({
        "organizationId": { "eq": "x" }
    }))
    .expect("where-input parses");
    assert_eq!(single_field_path(&where_input), vec![SNAKE.to_string()], "where-input surface");

    // 3 & 4. Aggregate `where` and `groupBy` fallback dimension.
    let metadata = empty_fact_metadata();
    let native = HashMap::new();
    let agg = crate::runtime::AggregateQueryParser::parse(
        &serde_json::json!({
            "table": "tf_x",
            "where": { "organizationId_eq": "x" },
            "groupBy": { "organizationId": true },
        }),
        &metadata,
        &native,
    )
    .expect("aggregate query parses");
    let agg_where = agg.where_clause.expect("aggregate where present");
    assert_eq!(
        single_field_path(&agg_where),
        vec![SNAKE.to_string()],
        "aggregate-where surface"
    );
    let dim = agg.group_by.iter().find_map(|g| match g {
        crate::compiler::aggregation::GroupBySelection::Dimension { path, alias } => {
            Some((path.clone(), alias.clone()))
        },
        _ => None,
    });
    let (dim_path, dim_alias) = dim.expect("groupBy dimension present");
    assert_eq!(dim_path, SNAKE, "aggregate-groupBy path surface");
    // The alias is the GraphQL response key (consumed verbatim by
    // AggregationProjector), so it keeps the camel surface name (#418/#410 rule).
    assert_eq!(dim_alias, "organizationId", "aggregate-groupBy alias keeps the surface name");

    // 5. Window `where`.
    let win = crate::runtime::WindowQueryParser::parse(
        &serde_json::json!({
            "table": "tf_x",
            "where": { "organizationId_eq": "x" },
        }),
        &metadata,
    )
    .expect("window query parses");
    let win_where = win.where_clause.expect("window where present");
    assert_eq!(single_field_path(&win_where), vec![SNAKE.to_string()], "window-where surface");
}

/// Minimal fact-table metadata with an empty dimension allowlist, so the
/// `groupBy` fallback dimension (priority 5) is the live path.
fn empty_fact_metadata() -> crate::compiler::fact_table::FactTableMetadata {
    serde_json::from_value(serde_json::json!({
        "table_name": "tf_x",
        "measures": [],
        "dimensions": { "name": "dimensions", "paths": [] },
        "denormalized_filters": []
    }))
    .expect("valid empty fact-table metadata")
}
