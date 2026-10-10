#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
//! #1391: a cascade mutation can take its cascade from `pg_tviews`' affected set.
//!
//! `cascade_source = "pg_tviews"` reaches the artifact; it needs a cascade to merge into, so
//! a mutation that is not `cascade` refuses it, and so does an unknown source.
//!
//! **Execution engine:** in-memory (no database: the refusals are the compiler's).

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::schema::CompiledSchema;
use serde_json::{Value, json};

fn schema_with(cascade: bool, source: &Value) -> Value {
    json!({
        "version": "2.0.0",
        "types": [{
            "name": "Post", "sql_source": "v_post",
            "fields": [{ "name": "id", "type": "ID", "nullable": false },
                       { "name": "title", "type": "String", "nullable": false }]
        }],
        "queries": [],
        "mutations": [{
            "name": "updatePost", "return_type": "Post", "cascade": cascade,
            "sql_source": "fn_update_post", "operation": "UPDATE",
            "cascade_source": source
        }]
    })
}

fn compile(schema: &Value) -> Result<CompiledSchema, String> {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("schema.json");
    std::fs::write(&path, schema.to_string()).unwrap();
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(compile_to_schema(CompileOptions {
            skip_hash: true,
            ..CompileOptions::new(path.to_str().unwrap())
        }))
        .map(|(compiled, _)| compiled.schema)
        .map_err(|e| format!("{e:#}"))
}

#[test]
fn a_pg_tviews_cascade_source_reaches_the_artifact() {
    let schema = compile(&schema_with(true, &json!("pg_tviews"))).unwrap();
    let mutation = schema.mutations.iter().find(|m| m.name == "updatePost").unwrap();
    assert_eq!(serde_json::to_value(mutation).unwrap()["cascade_source"], "pg_tviews");
    // The function's own cascade is the default, and is not written.
    let schema = compile(&schema_with(true, &json!("function"))).unwrap();
    let mutation = schema.mutations.iter().find(|m| m.name == "updatePost").unwrap();
    assert_eq!(serde_json::to_value(mutation).unwrap().get("cascade_source"), None);
}

#[test]
fn a_cascade_source_with_nothing_to_merge_into_fails_the_compile() {
    let Err(err) = compile(&schema_with(false, &json!("pg_tviews"))) else {
        panic!("compiled without cascade")
    };
    assert!(err.contains("cascade_source") && err.contains("`updatePost`"), "{err}");
    let Err(err) = compile(&schema_with(true, &json!("triggers"))) else {
        panic!("an unknown source compiled")
    };
    assert!(err.contains("triggers"), "{err}");
}

/// Provision `p1391_compile` (a view and a function that satisfy the mutation contract) in
/// `url`, then compile the `pg_tviews`-sourced schema against it.
async fn compile_against(url: &str) -> Result<(), String> {
    let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client
        .batch_execute(
            "DROP SCHEMA IF EXISTS p1391_compile CASCADE; CREATE SCHEMA p1391_compile;
             CREATE TYPE p1391_compile.mutation_response AS (succeeded boolean, state_changed
               boolean, error_class text, status_detail text, http_status smallint, message
               text, entity_id uuid, entity_type text, entity jsonb, updated_fields text[],
               cascade jsonb, error_detail jsonb, metadata jsonb);
             CREATE VIEW p1391_compile.v_post AS SELECT NULL::uuid AS id, NULL::jsonb AS data;
             CREATE FUNCTION p1391_compile.fn_update_post(p_id uuid, p_title text)
               RETURNS SETOF p1391_compile.mutation_response LANGUAGE sql AS
               $$ SELECT NULL::p1391_compile.mutation_response $$;",
        )
        .await
        .unwrap();
    let mut schema = schema_with(true, &json!("pg_tviews"));
    schema["types"][0]["sql_source"] = json!("p1391_compile.v_post");
    schema["mutations"][0]["sql_source"] = json!("p1391_compile.fn_update_post");
    schema["mutations"][0]["arguments"] = json!([
        { "name": "id", "type": "ID", "nullable": false },
        { "name": "title", "type": "String", "nullable": false }
    ]);
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("schema.json");
    std::fs::write(&path, schema.to_string()).unwrap();
    compile_to_schema(CompileOptions {
        skip_hash: true,
        database: Some(url),
        ..CompileOptions::new(path.to_str().unwrap())
    })
    .await
    .map(|_| ())
    .map_err(|e| format!("{e:#}"))
}

/// `compile --database` refuses a `pg_tviews` cascade source against a database that has no
/// `tviews.pg_tviews_flush_and_report()`: the mutation's first write would fail. The
/// function itself satisfies the contract, so the refusal is `pg_tviews`' alone.
#[tokio::test]
async fn compile_against_a_database_without_pg_tviews_refuses_the_source() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        return;
    };
    let Err(err) = compile_against(&url).await else {
        panic!("compiled against a database without pg_tviews")
    };
    assert!(
        err.contains("`updatePost`") && err.contains("pg_tviews_flush_and_report"),
        "{err}"
    );
}

/// Against a database that has `pg_tviews`, the same schema compiles.
#[tokio::test]
async fn compile_against_a_database_with_pg_tviews_accepts_the_source() {
    let Some(url) = std::env::var("TVIEWS_DATABASE_URL").ok().filter(|u| !u.is_empty()) else {
        return;
    };
    let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client.batch_execute("CREATE EXTENSION IF NOT EXISTS pg_tviews").await.unwrap();
    compile_against(&url).await.unwrap();
}

/// The suite's document compiles with no database.
#[test]
fn the_document_loads_without_a_database() {
    compile(&schema_with(true, &json!("pg_tviews"))).unwrap();
}
