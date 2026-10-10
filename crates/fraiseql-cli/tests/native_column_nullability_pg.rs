//! #1533: `compile --database` records whether a native column is `NOT NULL`, and only where
//! PostgreSQL proves it.
//!
//! An ordered relay page can seek its index with a row comparison only when no sort key can
//! be NULL. The proof is the catalog's: a base relation's `NOT NULL` column. A view reports
//! every column nullable on PostgreSQL 18 (`information_schema.columns.is_nullable = 'YES'`
//! even over a `NOT NULL` base column), and a native column inferred from an argument type
//! with no database has no proof at all: both record nullable.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** drops and recreates its own `p1533_native` schema → run
//! `--test-threads=1`.
#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr, clippy::panic)] // Reason: test code

use std::process::Command;

use serde_json::{Value, json};
use tempfile::TempDir;
use tokio_postgres::NoTls;

const SCHEMA: &str = "p1533_native";

async fn seed(url: &str) {
    let (client, conn) = tokio_postgres::connect(url, NoTls).await.unwrap();
    tokio::spawn(async move {
        let _ = conn.await;
    });
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {SCHEMA} CASCADE;
             CREATE SCHEMA {SCHEMA};
             CREATE TABLE {SCHEMA}.tv_word (
                 id uuid PRIMARY KEY, word text NOT NULL, note text, data jsonb NOT NULL);
             INSERT INTO {SCHEMA}.tv_word VALUES (gen_random_uuid(), 'a', NULL,
                 jsonb_build_object('word', 'a', 'note', NULL));
             CREATE VIEW {SCHEMA}.v_word AS SELECT id, word, note, data FROM {SCHEMA}.tv_word;"
        ))
        .await
        .unwrap();
}

/// `Word`, read by `words` (the table) and `wordsView` (the view over it), each taking a
/// `word` and a `note` argument, which is what makes them native columns.
fn schema() -> Value {
    let query = |name: &str, source: &str| {
        json!({
            "name": name, "return_type": "Word", "returns_list": true, "nullable": false,
            "sql_source": source,
            "arguments": [
                { "name": "word", "type": "String", "nullable": true },
                { "name": "note", "type": "String", "nullable": true },
                { "name": "id", "type": "UUID", "nullable": true }
            ]
        })
    };
    json!({
        "types": [{
            "name": "Word",
            "sql_source": format!("{SCHEMA}.tv_word"),
            "fields": [
                { "name": "id", "type": "UUID", "nullable": false },
                { "name": "word", "type": "String", "nullable": false },
                { "name": "note", "type": "String", "nullable": true }
            ]
        }],
        "queries": [
            query("words", &format!("{SCHEMA}.tv_word")),
            query("wordsView", &format!("{SCHEMA}.v_word"))
        ]
    })
}

/// Compile the schema, against the database when `url` is given; the compiled artifact.
fn compile(url: Option<&str>) -> Value {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("schema.json");
    std::fs::write(&input, schema().to_string()).unwrap();
    let out = dir.path().join("schema.compiled.json");
    let mut args = vec![
        "compile".to_string(),
        input.to_str().unwrap().to_string(),
        "--skip-hash".to_string(),
        "-o".to_string(),
        out.to_str().unwrap().to_string(),
    ];
    if let Some(url) = url {
        args.extend(["--database".to_string(), url.to_string()]);
    }
    let output = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli")).args(&args).output().unwrap();
    assert!(
        output.status.success(),
        "compile failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_str(&std::fs::read_to_string(out).unwrap()).unwrap()
}

/// `query`'s recorded native column `column`.
fn native(compiled: &Value, query: &str, column: &str) -> Value {
    let query = compiled["queries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|q| q["name"] == query)
        .unwrap_or_else(|| panic!("no query {query}"));
    query["native_columns"][column].clone()
}

#[tokio::test]
async fn a_tables_not_null_column_is_recorded_not_null_and_only_it() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #1533: DATABASE_URL not set");
        return;
    };
    seed(&url).await;
    let compiled = compile(Some(&url));

    assert_eq!(
        native(&compiled, "words", "word"),
        json!({ "pg_type": "text", "not_null": true })
    );
    assert_eq!(
        native(&compiled, "words", "note"),
        json!({ "pg_type": "text", "not_null": false })
    );
    assert_eq!(native(&compiled, "words", "id"), json!({ "pg_type": "uuid", "not_null": true }));
    // The view over the same table: PostgreSQL reports every view column nullable.
    for column in ["word", "note", "id"] {
        assert_eq!(native(&compiled, "wordsView", column)["not_null"], json!(false), "{column}");
    }
}

/// With no database, a `UUID` argument is still read as a native column, inferred from its
/// type; nothing proves it `NOT NULL`.
#[tokio::test]
async fn a_native_column_inferred_without_a_database_is_recorded_nullable() {
    let compiled = compile(None);
    assert_eq!(
        native(&compiled, "words", "id"),
        json!({ "pg_type": "uuid", "not_null": false })
    );
}
