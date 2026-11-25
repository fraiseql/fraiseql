//! `compile --database` refuses a schema declaring `[accent_insensitive]` against a database
//! that lacks its wrapper, or whose wrapper an index cannot use, naming the statement that
//! creates it; and compiles one whose wrapper is ready.
//!
//! The extension's own absence is covered by the unit tests of the report: the shared
//! database every suite runs against has `unaccent` created once, so it cannot be absent here.
//!
//! **Execution engine:** `PostgreSQL` with the `unaccent` extension · **Infrastructure:**
//! `DATABASE_URL` · **Parallelism:** each test owns its view and its wrapper's schema; the
//! extension is created under a transaction-scoped lock.

#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr, clippy::panic)]
// Reason: test code — panics and skip diagnostics are acceptable

use std::{io::Write, process::Command};

use serde_json::json;
use tempfile::{Builder, NamedTempFile, TempDir};
use tokio_postgres::NoTls;

/// Connect, create the extension and `test`'s view, and return the URL; `None` (skip)
/// without a database.
async fn database(test: &str) -> Option<String> {
    let url = fraiseql_test_support::try_database_url()?;
    let (client, conn) = match tokio_postgres::connect(&url, NoTls).await {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("skipping: cannot connect ({e})");
            return None;
        },
    };
    tokio::spawn(async move {
        let _ = conn.await;
    });
    client
        .batch_execute(&format!(
            "DO $do$ BEGIN \
               PERFORM pg_advisory_xact_lock(hashtext('accent_insensitive_against_db_pg')); \
               CREATE EXTENSION IF NOT EXISTS unaccent; \
             END $do$; \
             DROP SCHEMA IF EXISTS ai_db_{test} CASCADE; \
             CREATE SCHEMA ai_db_{test}; \
             DROP VIEW IF EXISTS v_ai_db_{test}; \
             CREATE VIEW v_ai_db_{test} AS \
               SELECT '1'::text AS id, '{{\"id\": \"1\", \"name\": \"Crème\"}}'::jsonb AS data;"
        ))
        .await
        .unwrap();
    Some(url)
}

/// Run `sql` against the database at `url`.
async fn execute(url: &str, sql: &str) {
    let (client, conn) = tokio_postgres::connect(url, NoTls).await.unwrap();
    tokio::spawn(async move {
        let _ = conn.await;
    });
    client.batch_execute(sql).await.unwrap();
}

/// `products` over `test`'s view, declaring the wrapper `ai_db_<test>.fold`.
fn schema_file(test: &str) -> NamedTempFile {
    let view = format!("v_ai_db_{test}");
    let schema = json!({
        "types": [{
            "name": "Product",
            "sql_source": view,
            "fields": [
                {"name": "id", "type": "ID", "nullable": false},
                {"name": "name", "type": "String", "nullable": false}
            ]
        }],
        "queries": [{
            "name": "products",
            "return_type": "Product",
            "returns_list": true,
            "sql_source": view
        }],
        "accent_insensitive": {"function": format!("ai_db_{test}.fold")}
    });
    let mut f = Builder::new().suffix(".json").tempfile().unwrap();
    f.write_all(serde_json::to_string_pretty(&schema).unwrap().as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

/// `fraiseql-cli compile <schema> --database <url>`: (success, combined output).
fn compile(test: &str, url: &str) -> (bool, String) {
    let schema = schema_file(test);
    let out_dir = TempDir::new().unwrap();
    let out = out_dir.path().join("schema.compiled.json");
    let output = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args([
            "compile",
            schema.path().to_str().unwrap(),
            "--database",
            url,
            "--skip-hash",
            "-o",
        ])
        .arg(&out)
        .output()
        .expect("failed to run fraiseql-cli");
    let log = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), log)
}

fn wrapper(test: &str, volatility: &str, returns: &str) -> String {
    format!(
        "CREATE FUNCTION ai_db_{test}.fold(text) RETURNS {returns} LANGUAGE sql {volatility} \
         RETURN public.unaccent('public.unaccent'::regdictionary, $1)"
    )
}

#[tokio::test]
async fn a_database_without_the_wrapper_is_refused_naming_its_statement() {
    let Some(url) = database("missing").await else {
        return;
    };
    let (ok, log) = compile("missing", &url);
    assert!(!ok, "{log}");
    assert!(log.contains("drift: 1 error(s)"), "the wrapper is the only drift: {log}");
    assert!(log.contains("no function ai_db_missing.fold(text)"), "{log}");
    assert!(
        log.contains("CREATE OR REPLACE FUNCTION \"ai_db_missing\".\"fold\"(text)"),
        "{log}"
    );
    assert!(log.contains("IMMUTABLE"), "{log}");
}

#[tokio::test]
async fn a_wrapper_that_is_not_immutable_is_refused() {
    let Some(url) = database("stable").await else {
        return;
    };
    execute(&url, &wrapper("stable", "STABLE", "text")).await;
    let (ok, log) = compile("stable", &url);
    assert!(!ok, "{log}");
    assert!(log.contains("drift: 1 error(s)"), "the wrapper is the only drift: {log}");
    assert!(log.contains("ai_db_stable.fold(text) is STABLE, not IMMUTABLE"), "{log}");
}

#[tokio::test]
async fn a_wrapper_that_does_not_return_text_is_refused() {
    let Some(url) = database("varchar").await else {
        return;
    };
    execute(&url, &wrapper("varchar", "IMMUTABLE", "varchar")).await;
    let (ok, log) = compile("varchar", &url);
    assert!(!ok, "{log}");
    assert!(log.contains("drift: 1 error(s)"), "the wrapper is the only drift: {log}");
    assert!(log.contains("returns character varying, not text"), "{log}");
}

#[tokio::test]
async fn a_ready_wrapper_compiles() {
    let Some(url) = database("ready").await else {
        return;
    };
    execute(&url, &wrapper("ready", "IMMUTABLE", "text")).await;
    let (ok, log) = compile("ready", &url);
    assert!(ok, "{log}");
    assert!(!log.contains("[accent_insensitive]"), "{log}");
}
