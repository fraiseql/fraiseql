//! A fact-table root's `where` and `having` operands are held to their published types before
//! any SQL runs, as a list query's `where` operands are.
//!
//! The aggregate and window roots' filters were bound as text and cast by the database, so an
//! operand of another type met the cast, whose error quoted it. Each is now refused by the
//! rule a built-in scalar's value is held to, naming the path to the operand and its type,
//! never the value, and no statement is sent. A value of the type is still read.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use serde_json::{Value, json};

use crate::{
    compiler::fact_table::{
        Additivity, DimensionColumn, FactRowScope, FactTableMetadata, FilterColumn, MeasureColumn,
        SqlType,
    },
    runtime::{Executor, executor::test_support::CapturingMockAdapter},
    schema::CompiledSchema,
};

const AGGREGATE: &str = "metric_aggregate";
const WINDOW: &str = "metric_window";

/// `tf_metric`: a filter column of every type a filter publishes a built-in scalar for, an
/// `int` and a `numeric` measure.
fn fact_table() -> FactTableMetadata {
    let filter = |name: &str, sql_type: SqlType| FilterColumn {
        name: name.to_string(),
        sql_type,
        indexed: false,
        hierarchy: None,
    };
    let measure = |name: &str, sql_type: SqlType| MeasureColumn {
        name: name.to_string(),
        sql_type,
        nullable: false,
        additivity: Additivity::Additive,
    };
    FactTableMetadata {
        table_name:               "tf_metric".to_string(),
        type_name:                None,
        row_scope:                FactRowScope::Server,
        inject_params:            indexmap::IndexMap::default(),
        measures:                 vec![
            measure("qty", SqlType::Int),
            measure("amount", SqlType::Decimal),
        ],
        dimensions:               DimensionColumn {
            name:  "data".to_string(),
            paths: vec![],
        },
        denormalized_filters:     vec![
            filter("n", SqlType::Int),
            filter("big", SqlType::BigInt),
            filter("ratio", SqlType::Float),
            filter("flag", SqlType::Boolean),
            filter("key", SqlType::Uuid),
            filter("at", SqlType::TimestampTz),
            filter("wall", SqlType::LocalTimestamp),
            filter("day", SqlType::Date),
            filter("label", SqlType::Text),
        ],
        calendar_dimensions:      vec![],
        native_measures:          std::collections::HashMap::new(),
        native_dimension_mapping: std::collections::HashMap::new(),
    }
}

/// The schema as a loaded one is: its fact-table roots and their inputs published.
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.add_fact_table("tf_metric".to_string(), fact_table());
    schema.build_indexes();
    schema
}

/// `root` with `arguments` written inline, its other arguments fixed.
fn document(root: &str, arguments: &str) -> String {
    if root == AGGREGATE {
        format!(
            "{{ {AGGREGATE}(groupBy: {{label: true}}, aggregates: [{{count: true}}, \
             {{qty_sum: true}}, {{amount_sum: true}}], {arguments}) {{ label count }} }}"
        )
    } else {
        format!(
            r#"{{ {WINDOW}(select: [{{type: "filter", name: "label", alias: "label"}}],
                 windows: [{{function: {{type: "row_number"}}, alias: "rank"}}], {arguments}) }}"#
        )
    }
}

/// Run `document` with `variables`: the refusal's message, or `None` when a statement was
/// sent. A refusal must send none.
async fn refusal(document: &str, variables: &Value) -> Option<String> {
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let executor = Executor::new(schema(), Arc::clone(&adapter));
    let message = match executor.execute(document, Some(variables)).await {
        Err(error) => Some(error.to_string()),
        Ok(response) => response["errors"][0]["message"].as_str().map(str::to_string),
    };
    let sql = adapter.captured_aggregate_sql();
    match &message {
        Some(message) => assert!(sql.is_none(), "{document}: refused after SQL: {message}"),
        None => assert!(sql.is_some(), "{document}: never read"),
    }
    message
}

/// Assert `document` is refused naming `path` and `scalar`, and quoting no operand.
async fn assert_refused(document: &str, variables: &Value, path: &str, scalar: &str) {
    let message = refusal(document, variables)
        .await
        .unwrap_or_else(|| panic!("{document}: served"));
    assert!(
        message.contains(&format!("`{path}` has type `{scalar}`")),
        "{document}: {message}"
    );
    for quoted in ["abc", "nope", "tomorrow", "not-a-uuid", "1.5", "300000"] {
        assert!(
            !message.contains(quoted),
            "{document}: the refusal quotes `{quoted}`: {message}"
        );
    }
}

/// Every built-in scalar a fact-table filter publishes, under a comparison, on both roots: a
/// value of another type is refused, one of the type is read. The window root's generator
/// runs `eq` and `neq` on a filter column, so its comparison is `eq`.
#[tokio::test]
async fn a_where_comparison_operand_is_held_to_its_columns_type() {
    for (field, scalar, refused, served) in [
        ("n", "Int", r#""abc""#, "7"),
        ("n", "Int", "1.5", "7"),
        ("big", "BigInt", r#""12abc""#, r#""9007199254740993""#),
        ("ratio", "Float", r#""1.5""#, "1.5"),
        ("flag", "Boolean", r#""nope""#, "true"),
        ("key", "UUID", r#""not-a-uuid""#, r#""00000000-0000-0000-0000-000000000001""#),
        ("at", "DateTime", r#""tomorrow""#, r#""2026-07-01T09:30:00Z""#),
        ("wall", "LocalDateTime", r#""tomorrow""#, r#""2026-07-01T09:30:00""#),
        ("day", "Date", r#""tomorrow""#, r#""2026-07-01""#),
        ("label", "String", "300000", r#""north""#),
    ] {
        for (root, op) in [(AGGREGATE, "gt"), (WINDOW, "eq")] {
            let op = if matches!(field, "flag" | "key") {
                "eq"
            } else {
                op
            };
            let at = |operand: &str| document(root, &format!("where: {{{field}_{op}: {operand}}}"));
            assert_refused(&at(refused), &json!({}), &format!("where.{field}_{op}"), scalar).await;
            assert_eq!(refusal(&at(served), &json!({})).await, None, "{}", at(served));
        }
    }
}

/// `_in` and `_nin` hold each item to the column's type, naming the item, on both roots.
#[tokio::test]
async fn a_where_list_operands_items_are_held_to_the_columns_type() {
    for root in [AGGREGATE, WINDOW] {
        assert_refused(
            &document(root, r#"where: {n_in: [1, "abc"]}"#),
            &json!({}),
            "where.n_in[1]",
            "Int",
        )
        .await;
        assert_refused(
            &document(root, r#"where: {key_nin: ["not-a-uuid"]}"#),
            &json!({}),
            "where.key_nin[0]",
            "UUID",
        )
        .await;
    }
    let served = r#"where: {n_in: [1, 2], key_nin: ["00000000-0000-0000-0000-000000000001"]}"#;
    assert_eq!(refusal(&document(AGGREGATE, served), &json!({})).await, None);
}

/// A null check takes a `Boolean`, and a pattern a `String`, whatever the column's type.
#[tokio::test]
async fn a_null_check_and_a_pattern_are_held_to_their_operators_types() {
    assert_refused(
        &document(AGGREGATE, r#"where: {n_isnull: "nope"}"#),
        &json!({}),
        "where.n_isnull",
        "Boolean",
    )
    .await;
    assert_refused(
        &document(AGGREGATE, "where: {label_contains: 300000}"),
        &json!({}),
        "where.label_contains",
        "String",
    )
    .await;
    let served = r#"where: {n_isnull: false, label_contains: "x"}"#;
    assert_eq!(refusal(&document(AGGREGATE, served), &json!({})).await, None);
}

/// A `having` operand is held to the type of the aggregate it compares: an `int` sum is a
/// `BigInt`, a `numeric` sum a `Decimal`, a count an `Int`.
#[tokio::test]
async fn a_having_operand_is_held_to_its_aggregates_type() {
    for (key, scalar, refused, served) in [
        ("qty_sum_gt", "BigInt", r#""abc""#, r#""4294967294""#),
        ("amount_sum_gt", "Decimal", "1.5", r#""12.50""#),
        ("amount_sum_lte", "Decimal", r#""abc""#, "12"),
        ("count_gte", "Int", r#""abc""#, "2"),
    ] {
        let at = |operand: &str| document(AGGREGATE, &format!("having: {{{key}: {operand}}}"));
        assert_refused(&at(refused), &json!({}), &format!("having.{key}"), scalar).await;
        assert_eq!(refusal(&at(served), &json!({})).await, None, "{}", at(served));
    }
}

/// A window function's own operands are held to their published types too.
#[tokio::test]
async fn a_window_functions_operand_is_held_to_its_type() {
    let refused = format!(
        r#"{{ {WINDOW}(select: [{{type: "filter", name: "label", alias: "label"}}],
             windows: [{{function: {{type: "ntile", n: "abc"}}, alias: "tile"}}]) }}"#
    );
    assert_refused(&refused, &json!({}), "windows[0].function.n", "Int").await;
}

/// A `where` or `having` carried by a variable is held to the same rule as one written inline.
#[tokio::test]
async fn a_filter_carried_by_a_variable_is_held_to_the_same_rule() {
    let aggregate = format!(
        "query Q($w: MetricAggregateWhere, $h: MetricAggregateHaving) {{ {AGGREGATE}(groupBy: \
         {{label: true}}, aggregates: [{{count: true}}], where: $w, having: $h) {{ count }} }}"
    );
    assert_refused(&aggregate, &json!({"w": {"day_eq": "tomorrow"}}), "where.day_eq", "Date").await;
    assert_refused(&aggregate, &json!({"h": {"count_gt": "abc"}}), "having.count_gt", "Int").await;
    assert_eq!(refusal(&aggregate, &json!({"w": {"day_eq": "2026-07-01"}})).await, None);
    // A variable named like the argument, the request's own spelling.
    let bare = format!("{{ {AGGREGATE} {{ count }} }}");
    let request = json!({"groupBy": {"label": true}, "aggregates": [{"count": true}],
                         "where": {"n_in": [1, "abc"]}});
    assert_refused(&bare, &request, "where.n_in[1]", "Int").await;
}

/// The embedder entries take a request with no document; it is held to the same rule.
#[tokio::test]
async fn an_embedders_request_is_held_to_the_same_rule() {
    let schema = schema();
    let metadata = schema.get_fact_table("tf_metric").unwrap().clone();
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let executor = Executor::new(schema, Arc::clone(&adapter));
    let aggregate = json!({"groupBy": {"label": true}, "aggregates": [{"count": true}],
                           "where": {"at_lt": "tomorrow"}});
    let refused = executor
        .execute_aggregate_query(&aggregate, AGGREGATE, &metadata)
        .await
        .expect_err("a DateTime operand of another format is refused");
    assert!(refused.to_string().contains("`where.at_lt` has type `DateTime`"), "{refused}");
    let window = json!({
        "select": [{"type": "filter", "name": "label", "alias": "label"}],
        "windows": [{"function": {"type": "row_number"}, "alias": "rank"}],
        "where": {"big_eq": "12abc"}
    });
    let refused = executor
        .execute_window_query(&window, WINDOW, &metadata)
        .await
        .expect_err("a BigInt operand of another shape is refused");
    assert!(refused.to_string().contains("`where.big_eq` has type `BigInt`"), "{refused}");
    assert_eq!(adapter.captured_aggregate_sql(), None, "a refused request sent a statement");
}

/// An embedder's schema need not hold the table it passes, nor name it `tf_<name>`: the
/// request is held to the arguments the metadata alone publishes, and refused naming the
/// root it came in under.
#[tokio::test]
async fn an_embedders_table_its_schema_does_not_publish_is_held_to_the_same_rule() {
    let mut metadata = fact_table();
    metadata.table_name = "v_metric".to_string();
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let executor = Executor::new(CompiledSchema::new(), Arc::clone(&adapter));
    let request = |filter: Value| json!({"groupBy": {"label": true}, "aggregates": [{"count": true}], "where": filter});
    let refused = executor
        .execute_aggregate_query(&request(json!({"n_in": [1, "abc"]})), "metrics", &metadata)
        .await
        .expect_err("an Int item of another type is refused");
    let message = refused.to_string();
    assert!(message.contains("Query.metrics at `where.n_in[1]` has type `Int`"), "{message}");
    assert!(!message.contains("abc"), "the refusal quotes the value: {message}");
    assert_eq!(adapter.captured_aggregate_sql(), None, "a refused request sent a statement");
    executor
        .execute_aggregate_query(&request(json!({"n_in": [1, 2]})), "metrics", &metadata)
        .await
        .expect("a request of the published types is read");
    assert!(adapter.captured_aggregate_sql().is_some(), "never read");
}
