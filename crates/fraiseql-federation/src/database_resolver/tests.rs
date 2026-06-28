#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::collections::HashMap;

use fraiseql_db::{DatabaseType, WhereClause, WhereOperator};
use serde_json::json;

use super::{build_select_fields, quote_relation, render_row_filter};
use crate::{
    selection_parser::FieldSelection,
    types::{FederatedType, KeyDirective},
};

#[test]
fn test_database_resolver_creation() {
    // Test that resolver can be created (mock adapter would be used)
    // Actual DB tests are in integration tests
}

#[test]
fn quote_relation_quotes_a_bare_view_name() {
    assert_eq!(quote_relation("v_organization").unwrap(), "\"v_organization\"");
}

#[test]
fn quote_relation_quotes_each_schema_qualified_segment() {
    // A qualified sql_source becomes "schema"."relation", not a single mis-quoted
    // identifier — the bug that made `schema.v_organization` unresolvable (#504).
    assert_eq!(quote_relation("app.v_organization").unwrap(), "\"app\".\"v_organization\"");
}

#[test]
fn quote_relation_rejects_unsafe_or_empty_segments() {
    // sql_source is compiler-authored, but the FROM relation is interpolated, so the
    // identifier guard is defense-in-depth.
    assert!(quote_relation("v_org; DROP TABLE x").is_err());
    assert!(quote_relation("app.").is_err());
    assert!(quote_relation(".v_org").is_err());
    assert!(quote_relation("").is_err());
}

fn user_type_with_inaccessible() -> FederatedType {
    FederatedType {
        name:                "User".to_string(),
        keys:                vec![KeyDirective {
            fields:     vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends:          false,
        external_fields:     vec!["externalOnly".to_string()],
        shareable_fields:    vec![],
        inaccessible_fields: vec!["password_hash".to_string()],
        field_directives:    HashMap::new(),
        type_shareable:      false,
    }
}

/// M-fed-select-list: `@inaccessible` / `@external` fields and injection-shaped
/// tokens are dropped from the SELECT list; key fields are always present.
#[test]
fn build_select_fields_drops_inaccessible_external_and_injection() {
    let fed_type = user_type_with_inaccessible();
    let selection = FieldSelection::new(vec![
        "name".to_string(),
        "password_hash".to_string(),  // @inaccessible -> dropped
        "externalOnly".to_string(),   // @external -> dropped
        "__typename".to_string(),     // GraphQL meta-field -> dropped
        "id, (SELECT 1)".to_string(), // not a plain identifier -> dropped
    ]);

    let fields = build_select_fields(&selection, &fed_type);

    assert!(fields.contains(&"name".to_string()), "exposed field kept");
    assert!(fields.contains(&"id".to_string()), "key field always present");
    assert!(
        !fields.contains(&"password_hash".to_string()),
        "@inaccessible field must never be selected"
    );
    assert!(
        !fields.contains(&"externalOnly".to_string()),
        "@external field must never be selected"
    );
    assert!(!fields.iter().any(|f| f == "__typename"), "__typename is not a stored column");
    assert!(
        !fields.iter().any(|f| f.contains("SELECT")),
        "injection-shaped token must be dropped"
    );
}

/// C1b/R1: the per-row enforcement predicate renders to a columnar `NativeField`
/// equality whose bind placeholder is numbered **after** the key IN-clause
/// parameters, so it can be added to the lookup without colliding.
#[test]
fn render_row_filter_offsets_placeholder_past_in_clause() {
    let filter = WhereClause::NativeField {
        column:   "tenant_id".to_string(),
        pg_cast:  String::new(),
        operator: WhereOperator::Eq,
        value:    json!("tenant-abc"),
    };

    // Two key params already bound ($1, $2) → the filter must use $3 (PostgreSQL).
    let (sql, params) = render_row_filter(&filter, DatabaseType::PostgreSQL, 2).unwrap();
    assert_eq!(sql, "\"tenant_id\" = $3");
    assert_eq!(params, vec![json!("tenant-abc")]);

    // No prior params → placeholder starts at $1.
    let (sql0, _) = render_row_filter(&filter, DatabaseType::PostgreSQL, 0).unwrap();
    assert_eq!(sql0, "\"tenant_id\" = $1");
}

/// A native column carrying a PostgreSQL cast renders the two-step `$N::text::<type>`
/// form so a text-bound value compares correctly against a typed (e.g. `uuid`) column.
#[test]
fn render_row_filter_applies_native_cast() {
    let filter = WhereClause::NativeField {
        column:   "tenant_id".to_string(),
        pg_cast:  "uuid".to_string(),
        operator: WhereOperator::Eq,
        value:    json!("11111111-1111-1111-1111-111111111111"),
    };
    let (sql, params) = render_row_filter(&filter, DatabaseType::PostgreSQL, 1).unwrap();
    assert_eq!(sql, "\"tenant_id\" = $2::text::uuid");
    assert_eq!(params.len(), 1);
}
