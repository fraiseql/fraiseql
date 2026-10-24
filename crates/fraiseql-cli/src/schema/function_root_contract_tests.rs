//! Unit tests for the function-root contract check (no database).

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use fraiseql_core::schema::{ArgumentDefinition, FieldType, InjectedParamSource, QueryDefinition};

use super::*;
use crate::schema::pg_catalog::OutColumn;

/// `lineItems(assetId:, where:, limit:)`, tenant-bound: two parameters, `assetId` then
/// `tenant_id`.
fn line_items() -> QueryDefinition {
    let mut query = QueryDefinition::new("lineItems", "LineItem")
        .returning_list()
        .with_sql_source_function("billing.fn_line_items");
    query.arguments = vec![
        ArgumentDefinition::new("assetId", FieldType::Uuid),
        ArgumentDefinition::optional("where", FieldType::Json),
        ArgumentDefinition::optional("limit", FieldType::Int),
    ];
    query
        .inject_params
        .insert("tenant_id".to_string(), InjectedParamSource::Jwt("tenant_id".to_string()));
    query
}

fn column(name: &str, type_name: &str) -> OutColumn {
    OutColumn {
        name:      name.to_string(),
        type_name: type_name.to_string(),
        is_enum:   false,
    }
}

/// `billing.fn_line_items(uuid, text) RETURNS TABLE(data jsonb) STABLE` — what `line_items`
/// reads.
fn stable_set_returning() -> PgFunction {
    PgFunction {
        schema:      "billing".to_string(),
        name:        "fn_line_items".to_string(),
        in_types:    vec!["uuid".to_string(), "text".to_string()],
        in_names:    vec![
            Some("p_asset_id".to_string()),
            Some("p_tenant_id".to_string()),
        ],
        out_columns: vec![column("data", "jsonb")],
        returns_set: true,
        volatility:  "s".to_string(),
        language:    "sql".to_string(),
        source:      String::new(),
    }
}

fn problems_with(function: PgFunction) -> Vec<String> {
    check_function_root(&line_items(), &[function])
}

#[test]
fn a_stable_set_returning_function_of_the_bound_arity_is_clean() {
    assert_eq!(problems_with(stable_set_returning()), Vec::<String>::new());
}

#[test]
fn an_immutable_function_is_clean() {
    let mut function = stable_set_returning();
    function.volatility = "i".to_string();
    assert_eq!(problems_with(function), Vec::<String>::new());
}

#[test]
fn a_missing_function_is_reported() {
    let problems = check_function_root(&line_items(), &[]);
    assert_eq!(problems.len(), 1);
    assert!(problems[0].contains("no function"), "{problems:?}");
}

#[test]
fn a_volatile_function_is_refused() {
    let mut function = stable_set_returning();
    function.volatility = "v".to_string();
    let problems = problems_with(function);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("VOLATILE"), "{problems:?}");
}

#[test]
fn a_function_returning_one_value_is_refused() {
    let mut function = stable_set_returning();
    function.returns_set = false;
    let problems = problems_with(function);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("returns a set"), "{problems:?}");
}

#[test]
fn a_function_of_another_arity_names_the_parameters_the_query_binds() {
    let mut function = stable_set_returning();
    function.in_types.pop();
    function.in_names.pop();
    let problems = problems_with(function);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("2 parameter"), "{problems:?}");
    assert!(problems[0].contains("assetId, tenant_id"), "{problems:?}");
    assert!(problems[0].contains("takes 1"), "{problems:?}");
}

#[test]
fn the_auto_params_are_not_counted_as_parameters() {
    let mut function = stable_set_returning();
    function.in_types.push("jsonb".to_string());
    function.in_names.push(Some("p_where".to_string()));
    assert!(!problems_with(function).is_empty(), "where shapes the read, it is never bound");
}

#[test]
fn a_function_without_the_document_column_is_refused() {
    let mut function = stable_set_returning();
    function.out_columns = vec![column("payload", "jsonb")];
    let problems = problems_with(function);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("data"), "{problems:?}");
}

#[test]
fn a_document_column_that_is_not_json_is_refused() {
    let mut function = stable_set_returning();
    function.out_columns = vec![column("data", "text")];
    let problems = problems_with(function);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("text"), "{problems:?}");
}

#[test]
fn two_overloads_of_the_bound_arity_are_refused_as_ambiguous() {
    let mut other = stable_set_returning();
    other.in_types = vec!["text".to_string(), "text".to_string()];
    let problems = check_function_root(&line_items(), &[stable_set_returning(), other]);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("overload"), "{problems:?}");
}

#[test]
fn only_the_overload_of_the_bound_arity_is_checked() {
    let mut unary = stable_set_returning();
    unary.in_types.pop();
    unary.volatility = "v".to_string();
    let problems = check_function_root(&line_items(), &[unary, stable_set_returning()]);
    assert_eq!(problems, Vec::<String>::new());
}

#[test]
fn a_relation_query_is_not_a_function_root() {
    let relation = QueryDefinition::new("lines", "LineItem").with_sql_source("v_line_item");
    let schema = fraiseql_core::schema::CompiledSchema {
        queries: vec![relation, line_items()],
        ..Default::default()
    };
    let roots: Vec<&str> = function_roots(&schema).map(|q| q.name.as_str()).collect();
    assert_eq!(roots, vec!["lineItems"]);
}
