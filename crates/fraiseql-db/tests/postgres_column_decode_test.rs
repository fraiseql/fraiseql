#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used, clippy::print_stdout, clippy::print_stderr)] // Reason: test code, panics are acceptable

//! Integration tests for PostgreSQL column decoding (TEXT[], ENUM, NULL) via
//! tokio-postgres `Row::get` — the decode substrate the adapter builds on.
//!
//! The adapter's own decode ladder (`decode_cell`, behind `execute_raw_query`) is
//! driven at the end of the file, with the log it writes captured (#1514).

use std::sync::{Arc, Mutex};

use fraiseql_db::{postgres::PostgresAdapter, traits::DatabaseAdapter};
use serde_json::json;

/// Connect to the harness-provided Postgres (Dagger-bound in CI; a local spawn with
/// the `local-testcontainers` feature). Returns the client plus the service guard,
/// which the caller holds so a locally-spawned container outlives the test.
async fn connect_pg() -> (tokio_postgres::Client, fraiseql_test_support::Service) {
    let svc = fraiseql_test_support::postgres()
        .await
        .expect("DATABASE_URL must be set (or enable fraiseql-test-support/local-testcontainers)");
    let (client, connection) = tokio_postgres::connect(svc.url(), tokio_postgres::NoTls)
        .await
        .expect("failed to connect");
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Connection error: {e}");
        }
    });
    (client, svc)
}

#[tokio::test]
async fn pg_decodes_text_array_columns() {
    let (client, _svc) = connect_pg().await;

    // Create table with TEXT[] column
    // #968: drop-first so the suite is rerun-safe on a shared database.
    client
        .execute("DROP TABLE IF EXISTS test_table CASCADE", &[])
        .await
        .expect("drop-first");
    client
        .execute(
            "CREATE TABLE test_table (
                id BIGINT PRIMARY KEY,
                field_names TEXT[]
            )",
            &[],
        )
        .await
        .expect("failed to create table");

    // Insert rows: one with non-NULL array, one with NULL array
    client
        .execute(
            "INSERT INTO test_table (id, field_names) VALUES ($1, $2)",
            &[&1i64, &vec!["name", "email"]],
        )
        .await
        .expect("failed to insert non-null array");

    client
        .execute(
            "INSERT INTO test_table (id, field_names) VALUES ($1, $2)",
            &[&2i64, &None::<Vec<&str>>],
        )
        .await
        .expect("failed to insert null array");

    // Query and verify row_to_map handles both cases
    let rows = client
        .query("SELECT id, field_names FROM test_table ORDER BY id", &[])
        .await
        .expect("failed to query");

    assert_eq!(rows.len(), 2, "expected 2 rows");

    // Row 1: non-NULL TEXT[] → should be JSON array
    let row1 = &rows[0];
    let id1: i64 = row1.get(0);
    let field_names1: Vec<String> = row1.get(1);
    assert_eq!(id1, 1);
    assert_eq!(field_names1, vec!["name", "email"]);

    // Row 2: NULL TEXT[] → should be handled gracefully
    let row2 = &rows[1];
    let id2: i64 = row2.get(0);
    let field_names2: Option<Vec<String>> = row2.get(1);
    assert_eq!(id2, 2);
    assert!(field_names2.is_none(), "NULL array should deserialize as None");
}

#[tokio::test]
async fn pg_decodes_enum_columns() {
    let (client, _svc) = connect_pg().await;

    // Create ENUM type and table
    // #968: drop-first so the suite is rerun-safe on a shared database.
    client
        .execute("DROP TABLE IF EXISTS test_enum_table CASCADE", &[])
        .await
        .expect("drop-first");
    client
        .execute("DROP TYPE IF EXISTS status_enum CASCADE", &[])
        .await
        .expect("drop-first");
    client
        .execute("CREATE TYPE status_enum AS ENUM ('active', 'inactive', 'pending')", &[])
        .await
        .expect("failed to create enum type");

    client
        .execute(
            "CREATE TABLE test_enum_table (
                id BIGINT PRIMARY KEY,
                status status_enum
            )",
            &[],
        )
        .await
        .expect("failed to create table");

    // Insert rows with ENUM values
    // Cast the text parameter to the enum type — tokio-postgres binds &str as TEXT,
    // which Postgres will not implicitly coerce to status_enum.
    client
        .execute(
            "INSERT INTO test_enum_table (id, status) VALUES ($1, $2::text::status_enum)",
            &[&1i64, &"active"],
        )
        .await
        .expect("failed to insert active status");

    client
        .execute(
            "INSERT INTO test_enum_table (id, status) VALUES ($1, $2::text::status_enum)",
            &[&2i64, &"pending"],
        )
        .await
        .expect("failed to insert pending status");

    // Query and verify ENUM values round-trip as strings. The enum is cast to text in
    // SQL because tokio-postgres' String decoder does not accept a custom enum OID.
    let rows = client
        .query("SELECT id, status::text FROM test_enum_table ORDER BY id", &[])
        .await
        .expect("failed to query");

    assert_eq!(rows.len(), 2);

    // Both ENUM values should deserialize as strings
    let status1: String = rows[0].get(1);
    let status2: String = rows[1].get(1);
    assert_eq!(status1, "active");
    assert_eq!(status2, "pending");
}

#[tokio::test]
async fn pg_decodes_mixed_types_with_nulls() {
    let (client, _svc) = connect_pg().await;

    // Create test table with multiple types
    // #968: drop-first so the suite is rerun-safe on a shared database.
    client
        .execute("DROP TABLE IF EXISTS mixed_types CASCADE", &[])
        .await
        .expect("drop-first");
    client
        .execute(
            "CREATE TABLE mixed_types (
                id BIGINT PRIMARY KEY,
                int_val INT,
                text_val TEXT,
                bool_val BOOL,
                json_val JSONB
            )",
            &[],
        )
        .await
        .expect("failed to create table");

    // Insert rows with various NULL combinations
    client
        .execute(
            "INSERT INTO mixed_types (id, int_val, text_val, bool_val, json_val)
             VALUES ($1, $2, $3, $4, $5)",
            &[&1i64, &42i32, &"hello", &true, &json!({"key": "value"})],
        )
        .await
        .expect("failed to insert row with all values");

    client
        .execute(
            "INSERT INTO mixed_types (id, int_val, text_val, bool_val, json_val)
             VALUES ($1, $2, $3, $4, $5)",
            &[
                &2i64,
                &None::<i32>,
                &None::<String>,
                &None::<bool>,
                &None::<serde_json::Value>,
            ],
        )
        .await
        .expect("failed to insert row with all nulls");

    // Query and verify all types are handled
    let rows = client
        .query(
            "SELECT id, int_val, text_val, bool_val, json_val FROM mixed_types ORDER BY id",
            &[],
        )
        .await
        .expect("failed to query");

    assert_eq!(rows.len(), 2);

    // Row 1: all non-NULL values
    let id1: i64 = rows[0].get(0);
    let int_val1: i32 = rows[0].get(1);
    let text_val1: String = rows[0].get(2);
    let bool_val1: bool = rows[0].get(3);
    let json_val1: serde_json::Value = rows[0].get(4);
    assert_eq!(id1, 1);
    assert_eq!(int_val1, 42);
    assert_eq!(text_val1, "hello");
    assert!(bool_val1);
    assert_eq!(json_val1, json!({"key": "value"}));

    // Row 2: all NULL values should be retrievable as Option::None
    let id2: i64 = rows[1].get(0);
    let int_val2: Option<i32> = rows[1].get(1);
    let text_val2: Option<String> = rows[1].get(2);
    let bool_val2: Option<bool> = rows[1].get(3);
    // A SQL NULL must be retrieved as Option::None — `get::<Value>` on a NULL panics.
    let json_val2: Option<serde_json::Value> = rows[1].get(4);
    assert_eq!(id2, 2);
    assert!(int_val2.is_none());
    assert!(text_val2.is_none());
    assert!(bool_val2.is_none());
    assert!(json_val2.is_none());
}

/// A `tracing` writer that keeps what it is given, so a test can read what was logged.
#[derive(Clone, Default)]
struct CapturedLog(Arc<Mutex<Vec<u8>>>);

impl CapturedLog {
    fn take(&self) -> String {
        String::from_utf8(std::mem::take(&mut *self.0.lock().unwrap())).unwrap()
    }
}

impl std::io::Write for CapturedLog {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Issue #1514: a SQL `NULL` decodes to JSON `null` whatever its type, and is not
/// reported as a type the adapter cannot represent.
///
/// Every branch of the decode ladder refuses a `NULL` (`WasNull`), so a `NULL` used to fall
/// through to the last branch, which logged a `warn` claiming type drift for ordinary
/// `text`, `uuid` or `jsonb` columns. The second half guards the other direction: a value
/// no branch decodes must still be reported.
#[tokio::test]
async fn a_null_cell_of_any_type_decodes_to_null_without_a_warning() {
    let svc = fraiseql_test_support::postgres()
        .await
        .expect("DATABASE_URL must be set (or enable fraiseql-test-support/local-testcontainers)");
    let adapter = PostgresAdapter::new(svc.url()).await.expect("adapter");
    for ddl in [
        "DROP TYPE IF EXISTS issue_1514_mood CASCADE",
        "CREATE TYPE issue_1514_mood AS ENUM ('calm')",
    ] {
        adapter.execute_raw_query(ddl).await.expect("enum type");
    }

    let log = CapturedLog::default();
    let writer = log.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || writer.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::WARN)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let columns = [
        ("t", "text"),
        ("i2", "int2"),
        ("i4", "int4"),
        ("i8", "int8"),
        ("f8", "float8"),
        ("b", "bool"),
        ("n", "numeric"),
        ("u", "uuid"),
        ("tz", "timestamptz"),
        ("ts", "timestamp"),
        ("d", "date"),
        ("ta", "text[]"),
        ("e", "issue_1514_mood"),
        ("j", "jsonb"),
        ("p", "point"),
    ];
    let select: Vec<String> =
        columns.iter().map(|(name, ty)| format!("NULL::{ty} AS {name}")).collect();
    let rows = adapter
        .execute_raw_query(&format!("SELECT {}", select.join(", ")))
        .await
        .expect("select nulls");

    for (name, ty) in columns {
        assert_eq!(rows[0][name], serde_json::Value::Null, "NULL::{ty} decodes to null");
    }
    assert_eq!(log.take(), "", "a NULL of any type is not a decode failure");

    let rows = adapter.execute_raw_query("SELECT '(1,2)'::point AS p").await.expect("point");
    assert_eq!(rows[0]["p"], serde_json::Value::Null);
    let logged = log.take();
    assert!(
        logged.contains(" WARN ") && logged.contains("column=p") && logged.contains("point"),
        "a non-NULL value no branch decodes is still reported: {logged:?}"
    );

    adapter
        .execute_raw_query("DROP TYPE IF EXISTS issue_1514_mood CASCADE")
        .await
        .expect("cleanup");
}
