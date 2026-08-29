//! A field declared `String` is returned as a string — even when its text
//! happens to parse as JSON (#1192) — against **real PostgreSQL**.
//!
//! Storing serialized JSON in a text column is ordinary: audit payloads, webhook
//! bodies, imported documents, anything round-tripped through a queue. The
//! projector re-parses a string value that decodes as an object or an array, on
//! the theory that "scalar strings won't parse as Object/Array, so this is safe
//! for all field types". That premise is false for exactly the rows this feature
//! exists to carry, and the result violates the schema the server publishes:
//! a typed client generated from it declares `jsonish: string` and receives an
//! object, per row, depending on whether that row's text happens to parse.
//!
//! The fixture is built so the two answers differ and a sibling pins the
//! control: `plain` holds ordinary text, `jsonish` holds `{"a": 1, "b": [2,3]}`
//! as text, and `listish` holds `[1,2,3]` as text. `jsonb_typeof` reports
//! `string` for all three in the stored `data`.
//!
//! # Running
//!
//! ```bash
//! DATABASE_URL=postgres://…  cargo test -p fraiseql-core --test string_field_fidelity_postgres
//! ```
//!
//! Runs in the Dagger `integration: postgres` leg via its `--test '*'` sweep.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
#![allow(clippy::print_stderr)] // Reason: skip diagnostic when no backing Postgres

use std::sync::Arc;

use fraiseql_core::{
    db::{postgres::PostgresAdapter, traits::DatabaseAdapter},
    runtime::Executor,
    schema::{CompiledSchema, FieldType},
};
use fraiseql_test_utils::schema_builder::{TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder};
use serde_json::Value;

const VIEW: &str = "v_string_fidelity_note";

const FIXTURE: &str = r#"
DROP TABLE IF EXISTS v_string_fidelity_note;
CREATE TABLE v_string_fidelity_note (pk_note bigint, data jsonb);
INSERT INTO v_string_fidelity_note (pk_note, data) VALUES
  (1, jsonb_build_object(
        'id', '11110000-0000-0000-0000-000000000001',
        'plain', 'hello',
        'jsonish', '{"a": 1, "b": [2,3]}',
        'listish', '[1,2,3]',
        'payload', '{"kept": true}',
        'blob', '{"kept": true}'));
"#;

/// Exactly what the `jsonish` column holds, and exactly what the response must
/// carry back.
const JSONISH: &str = r#"{"a": 1, "b": [2,3]}"#;
const LISTISH: &str = "[1,2,3]";

fn schema() -> CompiledSchema {
    let note = TestTypeBuilder::new("Note", VIEW)
        .with_simple_field("id", FieldType::Uuid)
        .with_simple_field("plain", FieldType::String)
        .with_simple_field("jsonish", FieldType::String)
        .with_simple_field("listish", FieldType::String)
        // The two types whose value space legitimately CONTAINS objects, so the
        // text-recovery re-parse must keep firing for them (#1192's own risk):
        // `Json`, and a project-defined custom scalar this module cannot
        // adjudicate.
        .with_simple_field("payload", FieldType::Json)
        .with_simple_field("blob", FieldType::Scalar("Blob".to_string()))
        .build();

    let notes = TestQueryBuilder::new("notes", "Note")
        .returns_list(true)
        .with_sql_source(VIEW)
        .build();

    TestSchemaBuilder::new().with_type(note).with_query(notes).build()
}

async fn executor() -> Option<Executor<PostgresAdapter>> {
    let pg = fraiseql_test_support::postgres().await?;
    let adapter = PostgresAdapter::new(pg.url()).await.expect("connect to the bound PostgreSQL");
    for stmt in FIXTURE.split(";\n") {
        if stmt.trim().is_empty() {
            continue;
        }
        adapter.execute_raw_query(stmt).await.expect("provision the #1192 fixture");
    }
    Some(Executor::new(schema(), Arc::new(adapter)))
}

async fn first_note(exec: &Executor<PostgresAdapter>, doc: &str) -> Value {
    let response = exec.execute(doc, None).await.expect("the query must resolve");
    response["data"]["notes"][0].clone()
}

/// The stored text must come back as text, not as the value it happens to encode.
#[tokio::test]
async fn a_string_field_holding_json_text_is_returned_as_a_string() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };

    let note = first_note(&exec, "{ notes { id plain jsonish listish } }").await;

    assert_eq!(
        note["jsonish"],
        Value::String(JSONISH.to_string()),
        "`jsonish` is declared String and holds JSON *text*; a client generated from this \
         schema declares it `string` and `JSON.parse`s it. Response was: {note}"
    );
    assert_eq!(
        note["listish"],
        Value::String(LISTISH.to_string()),
        "the array form of the same fault: {note}"
    );
}

/// **Control.** The sibling holding ordinary text was always correct, so a
/// failure above reads as "JSON-shaped text is coerced" rather than "String
/// fields are broken".
#[tokio::test]
async fn a_string_field_holding_ordinary_text_is_unaffected() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };

    let note = first_note(&exec, "{ notes { id plain } }").await;
    assert_eq!(note["plain"], Value::String("hello".to_string()), "response was: {note}");
}

/// **Control.** The response must keep its declared *type*, not merely its
/// characters: a client reading `typeof` has to see a string.
#[tokio::test]
async fn the_returned_value_is_a_json_string_not_an_object() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };

    let note = first_note(&exec, "{ notes { jsonish listish } }").await;
    assert!(
        note["jsonish"].is_string(),
        "expected a JSON string, got {}: {note}",
        if note["jsonish"].is_object() {
            "an object"
        } else {
            "neither"
        }
    );
    assert!(note["listish"].is_string(), "expected a JSON string: {note}");
}

/// **Control — the other direction.** The re-parse exists to recover a nested
/// value the SQL side extracted with `->>`, and narrowing it must not switch it
/// off where it is doing its job. `Json` holds arbitrary JSON by definition, and
/// a custom scalar's value space belongs to the project that declared it, so
/// neither is marked and both still parse.
#[tokio::test]
async fn json_and_custom_scalar_fields_still_recover_text_extracted_values() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };

    let note = first_note(&exec, "{ notes { payload blob } }").await;
    assert_eq!(
        note["payload"],
        serde_json::json!({"kept": true}),
        "a `Json` field must still be recovered from its text form: {note}"
    );
    assert_eq!(
        note["blob"],
        serde_json::json!({"kept": true}),
        "a custom scalar is not adjudicated, so its recovery is unchanged: {note}"
    );
}
