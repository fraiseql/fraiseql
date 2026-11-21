//! A `where` operand is held to its field's type before any SQL runs, written inline or
//! carried by a variable.
//!
//! A literal operand on a built-in scalar reached the database's cast unchecked
//! (`(data->>'n')::int4`), which refused it with an error quoting the value, while the same
//! operand in a scalar variable was refused naming the variable. Each is now refused by the
//! one rule a built-in scalar's value is held to, naming the path to the operand (so the
//! field and the operator) and the field's type, never the value, and the adapter is never
//! reached. A value of the type still reaches it.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use serde_json::{Value, json};

use crate::{
    runtime::{Executor, executor::test_support::CapturingMockAdapter},
    schema::CompiledSchema,
};

/// `items` over an `Item` with one field of every built-in scalar a filter is held to.
fn schema() -> CompiledSchema {
    let fields: Vec<Value> = [
        ("n", "Int"),
        ("big", "BigInt"),
        ("ratio", "Float"),
        ("price", "Decimal"),
        ("flag", "Boolean"),
        ("at", "DateTime"),
        ("wall", "LocalDateTime"),
        ("day", "Date"),
        ("opens", "Time"),
        ("key", "UUID"),
        ("label", "String"),
    ]
    .iter()
    .map(|(name, field_type)| json!({"name": name, "field_type": field_type, "nullable": true}))
    .collect();
    let doc = json!({
        "types": [{"name": "Item", "sql_source": "v_item", "fields": fields}],
        "queries": [{
            "name": "items", "return_type": "Item", "returns_list": true, "nullable": false,
            "sql_source": "v_item", "auto_params": {"has_where": true},
            "arguments": [{"name": "ns", "arg_type": {"List": {"item": "Int", "nullable_items": false}},
                           "nullable": true}]
        }]
    });
    CompiledSchema::from_json(&doc.to_string(), false).expect("fixture schema loads")
}

/// Run `query` with `variables`; the refusal's message, or `None` when the read reached the
/// adapter. A refusal must leave the adapter unread.
async fn refusal(query: &str, variables: Option<&Value>) -> Option<String> {
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let executor = Executor::new(schema(), Arc::clone(&adapter));
    let message = match executor.execute(query, variables).await {
        Err(error) => Some(error.to_string()),
        Ok(response) => response["errors"][0]["message"].as_str().map(str::to_string),
    };
    match &message {
        Some(message) => {
            assert!(adapter.captured_where().is_none(), "{query}: refused after SQL: {message}");
        },
        None => assert!(adapter.captured_where().is_some(), "{query}: never read"),
    }
    message
}

/// Assert `query` is refused naming `path` and `scalar`, and quoting none of `values`.
async fn assert_refused(query: &str, variables: Option<&Value>, path: &str, scalar: &str) {
    let message = refusal(query, variables).await.unwrap_or_else(|| panic!("{query}: served"));
    assert!(message.contains(&format!("`{path}`")), "{query}: no path `{path}`: {message}");
    assert!(message.contains(&format!("`{scalar}`")), "{query}: no type {scalar}: {message}");
    for quoted in [
        "abc",
        "12abc",
        "nope",
        "tomorrow",
        "9am",
        "not-a-uuid",
        "1.5",
        "300000",
    ] {
        assert!(!message.contains(quoted), "{query}: the refusal quotes `{quoted}`: {message}");
    }
}

/// Every built-in scalar the filter is held to, under a comparison operator: a value of
/// another type is refused, one of the type is read.
#[tokio::test]
async fn a_comparison_operand_is_held_to_its_fields_type() {
    for (field, scalar, refused, served) in [
        ("n", "Int", r#""abc""#, "7"),
        ("n", "Int", "1.5", "-7"),
        ("n", "Int", "3000000000", "2147483647"),
        ("big", "BigInt", r#""12abc""#, r#""9007199254740993""#),
        ("big", "BigInt", r#""9223372036854775808""#, "9223372036854775807"),
        ("ratio", "Float", r#""1.5""#, "1.5"),
        ("price", "Decimal", r#""abc""#, r#""12.50""#),
        ("price", "Decimal", "1.5", "12"),
        ("flag", "Boolean", r#""nope""#, "true"),
        ("at", "DateTime", r#""tomorrow""#, r#""2026-07-01T09:30:00Z""#),
        ("wall", "LocalDateTime", r#""tomorrow""#, r#""2026-07-01T09:30:00""#),
        ("day", "Date", r#""tomorrow""#, r#""2026-07-01""#),
        ("opens", "Time", r#""9am""#, r#""09:30:00""#),
        ("key", "UUID", r#""not-a-uuid""#, r#""00000000-0000-0000-0000-000000000001""#),
    ] {
        for op in ["eq", "gt"] {
            let query = |operand: &str| {
                format!("{{ items(where: {{{field}: {{{op}: {operand}}}}}) {{ n }} }}")
            };
            assert_refused(&query(refused), None, &format!("where.{field}.{op}"), scalar).await;
            assert_eq!(refusal(&query(served), None).await, None, "{}", query(served));
        }
    }
}

/// `in` and `nin` hold each item to the field's type, naming the item.
#[tokio::test]
async fn a_list_operands_items_are_held_to_the_fields_type() {
    assert_refused(
        r#"{ items(where: {n: {in: [1, "abc"]}}) { n } }"#,
        None,
        "where.n.in[1]",
        "Int",
    )
    .await;
    assert_refused(
        r#"{ items(where: {key: {nin: ["not-a-uuid"]}}) { n } }"#,
        None,
        "where.key.nin[0]",
        "UUID",
    )
    .await;
    let served = r#"{ items(where: {n: {in: [1, 2]}, key: {nin: ["00000000-0000-0000-0000-000000000001"]}}) { n } }"#;
    assert_eq!(refusal(served, None).await, None);
}

/// A null check takes a `Boolean`, and a pattern a `String`, whatever the field's type.
#[tokio::test]
async fn a_null_check_and_a_pattern_are_held_to_their_operators_types() {
    assert_refused(
        r#"{ items(where: {n: {isnull: "nope"}}) { n } }"#,
        None,
        "where.n.isnull",
        "Boolean",
    )
    .await;
    assert_refused(
        "{ items(where: {label: {contains: 300000}}) { n } }",
        None,
        "where.label.contains",
        "String",
    )
    .await;
    assert_eq!(
        refusal(r#"{ items(where: {n: {isnull: true}, label: {contains: "x"}}) { n } }"#, None)
            .await,
        None
    );
}

/// An operand under `_and`, `_or` and `_not` is held to the same rule.
#[tokio::test]
async fn an_operand_under_a_logical_operator_is_held_to_its_fields_type() {
    assert_refused(
        r#"{ items(where: {_or: [{n: {eq: 1}}, {n: {eq: "abc"}}]}) { n } }"#,
        None,
        "where._or[1].n.eq",
        "Int",
    )
    .await;
    assert_refused(
        r#"{ items(where: {_not: {day: {eq: "tomorrow"}}}) { n } }"#,
        None,
        "where._not.day.eq",
        "Date",
    )
    .await;
}

/// A whole `where` carried by a variable is held to the same rule as one written inline.
#[tokio::test]
async fn a_where_variable_is_held_to_the_same_rule() {
    let query = "query Q($w: ItemWhereInput) { items(where: $w) { n } }";
    assert_refused(query, Some(&json!({"w": {"n": {"eq": "abc"}}})), "where.n.eq", "Int").await;
    assert_eq!(refusal(query, Some(&json!({"w": {"n": {"eq": 7}}}))).await, None);
}

/// Every other transport takes a client's arguments through
/// [`QueryMatch::with_client_arguments`](crate::runtime::QueryMatch::with_client_arguments):
/// a REST `filter` document, a gRPC request. Its `where` is held to the same rule.
#[test]
fn a_transports_where_is_held_to_the_same_rule() {
    let schema = schema();
    let query_def = schema.find_query("items").unwrap().clone();
    let with_where = |filter: Value| {
        crate::runtime::QueryMatch::from_operation(
            query_def.clone(),
            vec!["n".to_string()],
            std::iter::once(("where".to_string(), filter)).collect(),
            None,
        )
        .unwrap()
        .with_client_arguments(&schema)
    };
    let message = with_where(json!({"day": {"in": ["2026-07-01", "tomorrow"]}}))
        .expect_err("a Date operand of another format is refused")
        .to_string();
    assert!(message.contains("`where.day.in[1]` has type `Date`"), "{message}");
    assert!(!message.contains("tomorrow"), "the refusal quotes the value: {message}");
    assert!(with_where(json!({"day": {"in": ["2026-07-01"]}})).is_ok());
}

/// A list argument's items are held to its item type too: the argument check adjudicates a
/// value written at the argument, and a list is not one.
#[tokio::test]
async fn a_list_arguments_items_are_held_to_their_type() {
    assert_refused(r#"{ items(ns: [1, "abc"]) { n } }"#, None, "ns[1]", "Int").await;
    assert_eq!(refusal("{ items(ns: [1, 2]) { n } }", None).await, None);
}
