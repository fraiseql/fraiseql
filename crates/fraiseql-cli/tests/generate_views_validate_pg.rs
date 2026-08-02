//! #821 — `generate-views --validate` must be a validator that can FAIL.
//!
//! The previous implementation checked only the view-name prefix and printed
//! "✓ View DDL is valid" for files `PostgreSQL` rejected outright (the
//! composition views contained a literal `{}` placeholder). `--validate` now
//! executes the generated DDL against a real database in a rolled-back
//! transaction, so this suite proves both directions:
//!
//! - a valid target (the source relation exists with the projected columns) validates and exits 0,
//!   committing nothing;
//! - a broken target (missing source relation) FAILS with a non-zero exit — the direction that used
//!   to be impossible.
//!
//! Self-skips when no `DATABASE_URL` is set.
//!
//! **Execution engine:** `PostgreSQL`
//! **Infrastructure:** `DATABASE_URL`

#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr, clippy::panic)]
// Reason: test code — panics and skip diagnostics are acceptable

use std::{io::Write, process::Command};

use fraiseql_core::schema::{CompiledSchema, FieldDefinition, FieldType, TypeDefinition};
use tempfile::{Builder, NamedTempFile};
use tokio_postgres::NoTls;

/// Source relation for the valid case; uniquely named to stay isolated from
/// the shared fixtures.
const SOURCE: &str = "v_gv821_source";

fn schema_file(sql_source: &str) -> NamedTempFile {
    let mut schema = CompiledSchema::new();
    let mut thing = TypeDefinition::new("Thing", sql_source);
    thing.jsonb_column = "data".to_string();
    thing.fields.push(FieldDefinition::new("id", FieldType::Id));
    schema.types.push(thing);

    let json = schema.to_json().expect("serialize schema");
    let mut f = Builder::new().suffix(".json").tempfile().unwrap();
    f.write_all(json.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

/// Run `fraiseql-cli generate-views … --validate`; returns (success, stdout, stderr).
fn run_validate(schema_path: &std::path::Path, view: &str, url: &str) -> (bool, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args([
            "generate-views",
            "-s",
            schema_path.to_str().unwrap(),
            "--entity",
            "Thing",
            "--view",
            view,
            "--validate",
        ])
        .env("DATABASE_URL", url)
        .output()
        .expect("spawn fraiseql-cli");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

async fn connect(url: &str) -> tokio_postgres::Client {
    let (client, connection) = tokio_postgres::connect(url, NoTls).await.expect("connect");
    tokio::spawn(async move {
        let _ = connection.await;
    });
    client
}

/// Valid DDL validates, exits 0, and commits nothing (rolled back).
#[tokio::test]
async fn validate_passes_on_a_backed_source_and_commits_nothing() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let client = connect(&url).await;
    client
        .batch_execute(&format!(
            "DROP VIEW IF EXISTS {SOURCE}; \
             CREATE VIEW {SOURCE} AS SELECT 1::bigint AS id, now() AS created_at, \
             now() AS updated_at, NULL::timestamptz AS archived_at;"
        ))
        .await
        .expect("create source view");

    let schema = schema_file(SOURCE);
    let (ok, stdout, stderr) = run_validate(schema.path(), "ta_gv821_stream", &url);
    assert!(
        ok,
        "--validate must pass on a backed source:\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("valid"), "success output should say so:\n{stdout}");

    // Rolled back: the generated view must NOT exist afterwards.
    let count: i64 = client
        .query_one("SELECT count(*) FROM pg_class WHERE relname = 'ta_gv821_stream'", &[])
        .await
        .expect("query pg_class")
        .get(0);
    assert_eq!(count, 0, "--validate must roll back — it created a relation");

    client.batch_execute(&format!("DROP VIEW IF EXISTS {SOURCE};")).await.ok();
}

/// DDL referencing a missing source relation must FAIL — the direction the
/// old prefix-only "validator" could never take.
#[tokio::test]
async fn validate_fails_on_a_missing_source() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let schema = schema_file("v_gv821_missing_source");
    let (ok, stdout, stderr) = run_validate(schema.path(), "ta_gv821_broken", &url);
    assert!(
        !ok,
        "--validate must fail when PostgreSQL rejects the DDL:\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        !stdout.contains("View DDL is valid"),
        "a failing validation must not claim validity:\n{stdout}"
    );
}
