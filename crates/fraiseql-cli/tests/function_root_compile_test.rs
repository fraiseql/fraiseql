#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::panic)] // Reason: test code, panics are the failure mechanism
//! A query read from a `STABLE` set-returning function, and every read it cannot serve.
//!
//! `sql_source_kind = "function"` says the query's `sql_source` names a function its arguments
//! bind to, rather than a relation. Its rows are filtered, ordered and paged like a view's, so
//! the declaration compiles with the auto-params a list query has. What it refuses is every read
//! that issues a statement of its own over a relation: those have nothing to read.
//!
//! Each refusal asserts the rule's own wording (`sql_source_kind = "function"`), so a test cannot
//! pass because some other check answered first.
//!
//! **Execution engine:** in-memory · **Infrastructure:** none · **Parallelism:** safe

use fraiseql_cli::schema::{ConvertOptions, SchemaConverter, intermediate::IntermediateSchema};
use fraiseql_core::schema::{PaginationOrder, SourceKind};
use serde_json::{Value, json};

fn corpus(queries: Value) -> Value {
    let mut schema = json!({
        "types": [{
            "name": "LineItem",
            "sql_source": "v_line_item",
            "is_input": false,
            "fields": [{"name": "label", "type": "String", "nullable": false},
                       {"name": "amount", "type": "Int", "nullable": false}]
        }],
        "mutations": [],
    });
    schema["queries"] = queries;
    schema
}

/// The canonical accepted declaration: a list read of one asset's lines, tenant-bound.
fn line_items() -> Value {
    json!([{
        "name": "lineItems",
        "return_type": "LineItem",
        "returns_list": true,
        "nullable": false,
        "sql_source": "billing.fn_line_items",
        "sql_source_kind": "function",
        "arguments": [{"name": "assetId", "type": "ID", "nullable": false}],
        "inject_params": {"tenant_id": "jwt:tenant_id"}
    }])
}

fn line_items_with(key: &str, value: Value) -> Value {
    let mut queries = line_items();
    queries[0][key] = value;
    queries
}

fn convert(corpus: Value) -> anyhow::Result<fraiseql_cli::schema::CompiledArtifact> {
    let intermediate: IntermediateSchema =
        serde_json::from_value(corpus).expect("the corpus must deserialize");
    SchemaConverter::convert_artifact(intermediate, &ConvertOptions::default())
}

fn compiled(corpus: Value) -> fraiseql_core::schema::QueryDefinition {
    let artifact = convert(corpus).expect("this declaration must compile");
    artifact.schema.queries.into_iter().find(|q| q.name == "lineItems").unwrap()
}

/// The refusal of the function-root rule, failing if the corpus compiled.
fn refused(corpus: Value) -> String {
    let message = match convert(corpus) {
        Ok(_) => panic!("this declaration must not compile"),
        Err(error) => format!("{error:#}"),
    };
    assert!(
        message.contains("sql_source_kind = \"function\""),
        "refused by another rule than the function root's: {message}"
    );
    message
}

// ── the declaration compiles, and reaches the artifact ───────────────────────────────────

#[test]
fn a_function_root_compiles_as_a_list_read_of_its_function() {
    let query = compiled(corpus(line_items()));
    assert_eq!(query.sql_source_kind, SourceKind::Function);
    assert_eq!(query.sql_source.as_deref(), Some("billing.fn_line_items"));
    assert!(query.inject_params.contains_key("tenant_id"), "bound as the last parameter");
    assert!(query.auto_params.has_where && query.auto_params.has_limit, "its rows are paged");
    assert_eq!(query.function_arity(), 2, "assetId, then tenant_id");
}

#[test]
fn a_function_root_derives_no_identity_order_its_function_cannot_return() {
    assert_eq!(compiled(corpus(line_items())).pagination_order, None);
}

#[test]
fn a_function_root_keeps_the_page_order_its_author_declared() {
    let query = compiled(corpus(line_items_with("pagination_order", json!("line_no"))));
    assert_eq!(query.pagination_order, Some(PaginationOrder::Column("line_no".to_string())));
}

#[test]
fn the_function_kind_is_spelled_out_in_the_artifact_and_the_relation_kind_is_not() {
    let function = serde_json::to_value(compiled(corpus(line_items()))).unwrap();
    assert_eq!(function["sql_source_kind"], "function", "{function}");

    let mut relation = line_items();
    relation[0].as_object_mut().unwrap().remove("sql_source_kind");
    relation[0]["sql_source"] = json!("v_line_item");
    let relation = serde_json::to_value(compiled(corpus(relation))).unwrap();
    assert!(relation.get("sql_source_kind").is_none(), "{relation}");
}

#[test]
fn an_unknown_source_kind_does_not_deserialize() {
    let result = serde_json::from_value::<IntermediateSchema>(corpus(line_items_with(
        "sql_source_kind",
        json!("procedure"),
    )));
    assert!(result.is_err(), "a kind nothing reads must not compile as a relation");
}

// ── the reads it cannot serve ────────────────────────────────────────────────────────────

#[test]
fn a_function_root_without_a_sql_source_is_refused() {
    let mut queries = line_items();
    queries[0].as_object_mut().unwrap().remove("sql_source");
    assert!(refused(corpus(queries)).contains("names no function"));
}

#[test]
fn relay_beside_a_function_root_is_refused() {
    let message = refused(corpus(line_items_with("relay", json!(true))));
    assert!(message.contains("relay = true"), "{message}");
}

#[test]
fn a_count_sibling_beside_a_function_root_is_refused() {
    let message = refused(corpus(line_items_with("count", json!(true))));
    assert!(message.contains("count = true"), "{message}");
}

#[test]
fn a_streamed_export_beside_a_function_root_is_refused() {
    let message = refused(corpus(line_items_with("rest_stream", json!(true))));
    assert!(message.contains("rest_stream = true"), "{message}");
}

#[test]
fn a_rest_route_beside_a_function_root_is_refused() {
    let message = refused(corpus(line_items_with("rest", json!({"path": "/line-items"}))));
    assert!(message.contains("a rest block"), "{message}");
}

#[test]
fn the_refuse_posture_asks_a_function_root_for_a_declared_order() {
    let mut refuse = corpus(line_items());
    refuse["query_defaults"] = json!({"where_clause": true, "order_by": true, "limit": true,
                                      "offset": true, "pagination_order": "refuse"});
    let message = refused(refuse.clone());
    assert!(message.contains("pagination_order = \"refuse\""), "{message}");

    refuse["queries"] = line_items_with("pagination_order", json!("line_no"));
    assert_eq!(
        compiled(refuse).pagination_order,
        Some(PaginationOrder::Column("line_no".to_string()))
    );
}
