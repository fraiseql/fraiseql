#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

//! #1567: an object literal keeps the key order it was written in.
//!
//! `orderBy: {ratio: ASC, current: DESC}` sorts by `ratio` first. The parser stores an
//! object literal as a map ordered by key, so the literal was applied as
//! `{current: DESC, ratio: ASC}` while the same object passed as a variable (JSON, which
//! keeps its order) was applied as written: one request, two answers. The rows make the two
//! orders disagree on the first page: the smallest ratios are not current.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tv_p1567_doc` table.

use std::sync::Arc;

use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::{CompiledSchema, FieldType},
};
use fraiseql_test_utils::schema_builder::{TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder};
use serde_json::{Value, json};

const TABLE: &str = "tv_p1567_doc";

/// `(id, ratio, current)`: by `ratio` first the page is `a, b`; by `current` first, `c, d`.
const ROWS: [(&str, f64, bool); 4] = [
    ("a", 0.1, false),
    ("b", 0.2, false),
    ("c", 0.3, true),
    ("d", 0.4, true),
];

fn schema() -> CompiledSchema {
    let doc = TestTypeBuilder::new("Doc", TABLE)
        .with_simple_field("id", FieldType::Id)
        .with_simple_field("ratio", FieldType::Float)
        .with_simple_field("current", FieldType::Boolean)
        .build();
    let mut docs = TestQueryBuilder::new("docs", "Doc")
        .returns_list(true)
        .with_sql_source(TABLE)
        .build();
    docs.auto_params.has_order_by = true;
    docs.auto_params.has_limit = true;
    docs.nullable = false;
    let mut schema = TestSchemaBuilder::new().with_type(doc).with_query(docs).build();
    schema.build_indexes();
    schema
}

async fn executor() -> Option<Executor> {
    let url = fraiseql_test_support::try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    let values: Vec<String> = ROWS
        .iter()
        .map(|(id, ratio, current)| {
            format!("('{}'::jsonb)", json!({ "id": id, "ratio": ratio, "current": current }))
        })
        .collect();
    for ddl in [
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!(
            "CREATE TABLE {TABLE} (data jsonb, id text GENERATED ALWAYS AS (data->>'id') STORED)"
        ),
        format!("INSERT INTO {TABLE} VALUES {}", values.join(", ")),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    Some(Executor::new(schema(), Arc::new(adapter)))
}

/// The ids of the page `query` answers.
async fn page(executor: &Executor, query: &str, variables: Option<&Value>) -> Vec<String> {
    let response = executor
        .execute(query, variables)
        .await
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    response["data"]["docs"]
        .as_array()
        .unwrap_or_else(|| panic!("{query}: {response}"))
        .iter()
        .map(|d| d["id"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn an_order_by_literal_sorts_by_its_keys_in_the_order_written() {
    let Some(executor) = executor().await else {
        eprintln!("skipping #1567: DATABASE_URL not set");
        return;
    };

    let literal = page(
        &executor,
        "{ docs(limit: 2, orderBy: {ratio: ASC, current: DESC}) { id } }",
        None,
    )
    .await;

    assert_eq!(literal, ["a", "b"], "sorted by ratio first, as written");
}

#[tokio::test]
async fn an_order_by_literal_and_the_same_variable_return_the_same_rows() {
    let Some(executor) = executor().await else {
        eprintln!("skipping #1567: DATABASE_URL not set");
        return;
    };

    for written in ["{ratio: ASC, current: DESC}", "{current: DESC, ratio: ASC}"] {
        let literal =
            page(&executor, &format!("{{ docs(limit: 2, orderBy: {written}) {{ id }} }}"), None)
                .await;
        let as_json = written.replace("ratio", "\"ratio\"").replace("current", "\"current\"");
        let as_json = as_json.replace("ASC", "\"ASC\"").replace("DESC", "\"DESC\"");
        let variables = json!({ "o": serde_json::from_str::<Value>(&as_json).unwrap() });
        let variable = page(
            &executor,
            "query($o: JSON) { docs(limit: 2, orderBy: $o) { id } }",
            Some(&variables),
        )
        .await;
        assert_eq!(literal, variable, "{written}");
    }
}

/// A variable's default is a literal too, and keeps its order.
#[tokio::test]
async fn a_variable_default_keeps_its_order() {
    let Some(executor) = executor().await else {
        eprintln!("skipping #1567: DATABASE_URL not set");
        return;
    };

    // The second order is not the rows' own: a default that did not reach the query would
    // leave them unsorted, `a, b`, and pass the first case by coincidence.
    for (query, expected) in [
        (
            "query($o: JSON = {ratio: ASC, current: DESC}) { docs(limit: 2, orderBy: $o) { id } }",
            ["a", "b"],
        ),
        (
            "query($o: JSON = {current: DESC, ratio: ASC}) { docs(limit: 2, orderBy: $o) { id } }",
            ["c", "d"],
        ),
    ] {
        assert_eq!(page(&executor, query, None).await, expected, "{query}");
    }
}

/// An input object naming a key twice is refused (GraphQL § 5.6.3, Input Object Field
/// Uniqueness), never applied with one of its two values: `{ratio: ASC, ratio: DESC}` has no
/// order to sort by.
#[tokio::test]
async fn an_input_object_naming_a_key_twice_is_refused() {
    let Some(executor) = executor().await else {
        eprintln!("skipping #1567: DATABASE_URL not set");
        return;
    };

    for query in [
        "{ docs(limit: 2, orderBy: {ratio: ASC, ratio: DESC}) { id } }",
        "query($o: JSON = {current: DESC, ratio: ASC, current: ASC}) { docs(orderBy: $o) { id } }",
    ] {
        let error = executor
            .execute(query, None)
            .await
            .map(|response| panic!("{query} was answered: {response}"))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("names `") && error.contains("twice") && error.contains("5.6.3"),
            "{query}: {error}"
        );
    }
}
