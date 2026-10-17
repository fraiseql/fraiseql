//! What an author declares in `schema.json` is what the runtime serves, end to end.
//!
//! Each test authors a schema the way an SDK exports it, compiles it with the real
//! `fraiseql compile`, and runs operations through `fraiseql query` (the real `Executor` and
//! `PostgresAdapter`) against rows stored in PostgreSQL. The declaration is the contract: a
//! shape the compiler accepts must reach the response as declared, not tightened or loosened
//! on the way.
//!
//! - #1586: a list declared with nullable items (`[Float]`) serves a `null` item. A list declared
//!   with non-null items (`[Float!]`) still nulls the list on one (§ 6.4.4, #1522).
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** drops and recreates its own `authored_contract` schema → run
//! `--test-threads=1`.
#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr, clippy::panic)] // Reason: test code

use std::{path::Path, process::Command};

use serde_json::{Value, json};
use tempfile::TempDir;
use tokio_postgres::NoTls;

const SCHEMA: &str = "authored_contract";

async fn execute(url: &str, sql: &str) {
    let (client, conn) = tokio_postgres::connect(url, NoTls).await.unwrap();
    tokio::spawn(async move {
        let _ = conn.await;
    });
    client.batch_execute(sql).await.unwrap();
}

/// Compile `schema` into `dir`; the compiled artifact's path.
fn compile(dir: &Path, schema: &Value) -> std::path::PathBuf {
    let input = dir.join("schema.json");
    std::fs::write(&input, schema.to_string()).unwrap();
    let out = dir.join("schema.compiled.json");
    let output = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args([
            "compile",
            input.to_str().unwrap(),
            "--skip-hash",
            "-o",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "compile failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    out
}

/// Run `operation` through `fraiseql query`: the response, whatever its exit code.
fn query(compiled: &Path, url: &str, operation: &str, variables: Option<&Value>) -> Value {
    let mut args = vec![
        "query".to_string(),
        operation.to_string(),
        "--schema".to_string(),
        compiled.to_str().unwrap().to_string(),
        "--database".to_string(),
        url.to_string(),
    ];
    if let Some(variables) = variables {
        args.extend(["--variables".to_string(), variables.to_string()]);
    }
    let output = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli")).args(&args).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!(
            "`fraiseql query` printed no JSON response ({e}):\nstdout: {stdout}\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

// ── #1586: list item nullability ────────────────────────────────────────────────────

/// `Report.monthlyValues: [Float]` (a month with no data is `null`) and
/// `Report.strictValues: [Float!]`, over one row holding a `null` in each. The row stores
/// its keys in snake case, as the compiler's default naming convention reads them.
async fn reports(url: &str, dir: &Path) -> std::path::PathBuf {
    execute(
        url,
        &format!(
            "DROP SCHEMA IF EXISTS {SCHEMA} CASCADE;
             CREATE SCHEMA {SCHEMA};
             CREATE TABLE {SCHEMA}.tb_report (id uuid PRIMARY KEY, data jsonb NOT NULL);
             INSERT INTO {SCHEMA}.tb_report VALUES ('00000000-0000-0000-0000-000000001586',
                 '{{\"id\": \"00000000-0000-0000-0000-000000001586\",
                    \"monthly_values\": [12.0, null, 7.5], \"strict_values\": [1.0, null]}}');
             CREATE VIEW {SCHEMA}.v_report AS SELECT id, data FROM {SCHEMA}.tb_report;"
        ),
    )
    .await;
    compile(
        dir,
        &json!({
            "types": [{
                "name": "Report",
                "sql_source": format!("{SCHEMA}.v_report"),
                "fields": [
                    { "name": "id", "type": "ID", "nullable": false },
                    { "name": "monthlyValues", "type": "[Float]", "nullable": true },
                    { "name": "strictValues", "type": "[Float!]", "nullable": true }
                ]
            }],
            "queries": [{
                "name": "reports", "return_type": "Report", "returns_list": true,
                "nullable": false, "sql_source": format!("{SCHEMA}.v_report")
            }]
        }),
    )
}

#[tokio::test]
async fn a_list_declared_with_nullable_items_serves_a_null_item() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #1586: DATABASE_URL not set");
        return;
    };
    let dir = TempDir::new().unwrap();
    let compiled = reports(&url, dir.path()).await;

    let response = query(&compiled, &url, "{ reports { monthlyValues } }", None);

    assert_eq!(
        response,
        json!({ "data": { "reports": [{ "monthlyValues": [12.0, null, 7.5] }] } }),
        "a null item of a `[Float]` list is a value the field may take"
    );
}

#[tokio::test]
async fn a_list_declared_with_non_null_items_is_nulled_by_a_null_item() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #1586: DATABASE_URL not set");
        return;
    };
    let dir = TempDir::new().unwrap();
    let compiled = reports(&url, dir.path()).await;

    let response = query(&compiled, &url, "{ reports { strictValues } }", None);

    assert_eq!(response["data"], json!({ "reports": [{ "strictValues": null }] }));
    assert_eq!(response["errors"][0]["path"], json!(["reports", 0, "strictValues", 1]));
}

#[tokio::test]
async fn the_published_schema_carries_the_declared_item_nullability() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #1586: DATABASE_URL not set");
        return;
    };
    let dir = TempDir::new().unwrap();
    let compiled = reports(&url, dir.path()).await;

    let response = query(
        &compiled,
        &url,
        "{ __type(name: \"Report\") { fields { name type { kind ofType { kind name ofType { name } } } } } }",
        None,
    );

    let field = |name: &str| {
        response["data"]["__type"]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == name)
            .unwrap_or_else(|| panic!("no field {name}: {response}"))["type"]
            .clone()
    };
    let monthly = field("monthlyValues");
    assert_eq!(
        (&monthly["kind"], &monthly["ofType"]["kind"], &monthly["ofType"]["name"]),
        (&json!("LIST"), &json!("SCALAR"), &json!("Float")),
        "`[Float]`: the items are nullable: {monthly}"
    );
    let strict = field("strictValues");
    assert_eq!(
        (&strict["kind"], &strict["ofType"]["kind"], &strict["ofType"]["ofType"]["name"]),
        (&json!("LIST"), &json!("NON_NULL"), &json!("Float")),
        "`[Float!]`: {strict}"
    );
}
