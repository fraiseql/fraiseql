//! § 5.8.5 (All Variable Usages Are Allowed), nullability: a variable declared nullable is
//! refused where its position is non-null, for the document alone, against real PostgreSQL.
//!
//! Each refusal is paired with the control that declares the variable at the published type
//! and reads the rows, so the rule is shown refusing exactly the declaration and nothing else.

use std::sync::Arc;

use fraiseql_core::{
    db::{postgres::PostgresAdapter, traits::DatabaseAdapter},
    error::FraiseQLError,
    runtime::Executor,
    schema::{ArgumentDefinition, AutoParams, CompiledSchema, FieldType},
};
use fraiseql_test_utils::schema_builder::{TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder};
use serde_json::{Value, json};

use super::{FIXTURE, VIEW};

const FIRST: &str = "11110000-0000-0000-0000-000000000001";

fn schema() -> CompiledSchema {
    let thing_type = TestTypeBuilder::new("Thing", VIEW)
        .with_simple_field("id", FieldType::Uuid)
        .with_simple_field("name", FieldType::String)
        .build();
    let mut things = TestQueryBuilder::new("things", "Thing")
        .returns_list(true)
        .with_sql_source(VIEW)
        .build();
    things.auto_params = AutoParams::all();
    let mut thing = TestQueryBuilder::new("thing", "Thing").with_sql_source(VIEW).build();
    thing.arguments = vec![ArgumentDefinition::new("id", FieldType::Uuid)];
    // The shape the Python SDK's RAG source sends: a required search text and a limit.
    let mut documents = TestQueryBuilder::new("documents", "Thing")
        .returns_list(true)
        .with_sql_source(VIEW)
        .build();
    documents.auto_params = AutoParams::all();
    documents.arguments = vec![ArgumentDefinition::new("name", FieldType::String)];

    TestSchemaBuilder::new()
        .with_type(thing_type)
        .with_query(things)
        .with_query(thing)
        .with_query(documents)
        .build()
}

async fn executor() -> Option<Executor> {
    let pg = fraiseql_test_support::postgres().await?;
    let adapter = PostgresAdapter::new(pg.url()).await.expect("connect to the bound PostgreSQL");
    for stmt in FIXTURE.split(";\n").filter(|s| !s.trim().is_empty()) {
        adapter.execute_raw_query(stmt).await.expect("provision the fixture");
    }
    Some(Executor::new(schema(), Arc::new(adapter)))
}

/// The names the root answered, or the refusal's message.
async fn names(
    exec: &Executor,
    document: &str,
    variables: Option<&Value>,
) -> Result<Vec<String>, String> {
    match exec.execute(document, variables).await {
        Ok(response) => {
            let data = &response["data"];
            let root =
                data.as_object().and_then(|d| d.values().next()).cloned().unwrap_or_default();
            let rows = match root {
                Value::Array(rows) => rows,
                Value::Null => Vec::new(),
                row => vec![row],
            };
            let mut names: Vec<String> = rows
                .iter()
                .map(|r| r["name"].as_str().unwrap_or_default().to_string())
                .collect();
            names.sort();
            Ok(names)
        },
        Err(FraiseQLError::Validation { message, .. }) => Err(message),
        Err(other) => panic!("expected rows or a validation refusal, got {other:?}"),
    }
}

macro_rules! require_pg {
    () => {
        match executor().await {
            Some(exec) => exec,
            None => {
                eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
                return;
            },
        }
    };
}

#[tokio::test]
async fn a_nullable_variable_at_a_required_argument_is_refused() {
    let exec = require_pg!();
    let vars = json!({ "id": FIRST });

    let refused = names(&exec, "query Q($id: UUID) { thing(id: $id) { name } }", Some(&vars)).await;
    let served = names(&exec, "query Q($id: UUID!) { thing(id: $id) { name } }", Some(&vars)).await;

    assert_eq!(
        refused,
        Err(
            "Variable `$id` is declared as `UUID` but is used for argument `id` on Query.thing, \
             which has type `UUID!`"
                .to_string()
        )
    );
    assert_eq!(served, Ok(vec!["first".to_string()]));
}

/// A non-null default stands in for an absent value, so the variable cannot carry a null to
/// the argument; a `null` default can, and is refused.
#[tokio::test]
async fn a_variable_default_decides_whether_a_nullable_variable_fits() {
    let exec = require_pg!();

    let defaulted = names(
        &exec,
        &format!(r#"query Q($id: UUID = "{FIRST}") {{ thing(id: $id) {{ name }} }}"#),
        None,
    )
    .await;
    let null_default =
        names(&exec, "query Q($id: UUID = null) { thing(id: $id) { name } }", None).await;

    assert_eq!(defaulted, Ok(vec!["first".to_string()]));
    let message = null_default.expect_err("a null default can reach the required argument");
    assert!(message.starts_with("Variable `$id` is declared as `UUID` but"), "{message}");
}

/// An `in` operand is `[String!]`: a list variable with nullable items is refused where it is
/// nested in the `where` literal, and one with non-null items filters.
#[tokio::test]
async fn a_list_variable_with_nullable_items_is_refused_at_a_non_null_item_list() {
    let exec = require_pg!();
    let vars = json!({ "n": ["first", "second"] });

    let refused = names(
        &exec,
        "query Q($n: [String]) { things(where: {name: {in: $n}}) { name } }",
        Some(&vars),
    )
    .await;
    let served = names(
        &exec,
        "query Q($n: [String!]) { things(where: {name: {in: $n}}) { name } }",
        Some(&vars),
    )
    .await;

    assert_eq!(
        refused,
        Err("Variable `$n` is declared as `[String]` but is used at `where.name.in` on \
             Query.things, which has type `[String!]`"
            .to_string())
    );
    assert_eq!(served, Ok(vec!["first".to_string(), "second".to_string()]));
}

/// A variable written as an item of a list literal stands at the item's type.
#[tokio::test]
async fn a_nullable_variable_as_a_non_null_list_item_is_refused() {
    let exec = require_pg!();
    let vars = json!({ "n": "second" });

    let refused = names(
        &exec,
        r#"query Q($n: String) { things(where: {name: {in: ["first", $n]}}) { name } }"#,
        Some(&vars),
    )
    .await;
    let served = names(
        &exec,
        r#"query Q($n: String!) { things(where: {name: {in: ["first", $n]}}) { name } }"#,
        Some(&vars),
    )
    .await;

    assert_eq!(
        refused,
        Err("Variable `$n` is declared as `String` but is used at `where.name.in[1]` on \
             Query.things, which has type `String!`"
            .to_string())
    );
    assert_eq!(served, Ok(vec!["first".to_string(), "second".to_string()]));
}

/// The converse is allowed: a non-null variable fits a nullable position, at the argument and
/// at a list's items.
#[tokio::test]
async fn a_non_null_variable_fits_a_nullable_position() {
    let exec = require_pg!();
    let vars = json!({ "l": 2, "n": ["first", "second", "third"] });

    let served = names(
        &exec,
        "query Q($l: Int!, $n: [String!]!) { things(limit: $l, where: {name: {in: $n}}, \
         orderBy: {name: ASC}) { name } }",
        Some(&vars),
    )
    .await;

    assert_eq!(served, Ok(vec!["first".to_string(), "second".to_string()]));
}

/// The document the Python SDK's RAG source builds with `search_type="String!"` is served at
/// a required text argument; with its default `String` it is refused.
#[tokio::test]
async fn the_rag_source_document_fits_a_required_text_argument_when_declared_non_null() {
    let exec = require_pg!();
    let vars = json!({ "name": "second", "limit": 10 });

    let served = names(
        &exec,
        "query ($name: String!, $limit: Int) { documents(name: $name, limit: $limit) { name } }",
        Some(&vars),
    )
    .await;
    let refused = names(
        &exec,
        "query ($name: String, $limit: Int) { documents(name: $name, limit: $limit) { name } }",
        Some(&vars),
    )
    .await;

    assert_eq!(served, Ok(vec!["second".to_string()]));
    assert_eq!(
        refused,
        Err("Variable `$name` is declared as `String` but is used for argument `name` on \
             Query.documents, which has type `String!`"
            .to_string())
    );
}
