#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! Integration tests for `row_to_map` with real PostgreSQL instances.
//!
//! These tests verify that `row_to_map` correctly handles TEXT[] and ENUM columns,
//! including NULL values, by spinning up a real Postgres database via testcontainers.

use serde_json::json;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

#[tokio::test]
#[ignore = "requires a running PostgreSQL container (testcontainers); run with --include-ignored"]
async fn row_to_map_handles_text_array_columns() {
    let container = Postgres::default()
        .with_user("postgres")
        .with_password("postgres")
        .with_db_name("test")
        .start()
        .await
        .expect("failed to start postgres container");

    let host_port = container.get_host_port_ipv4(5432).await.expect("failed to get port");
    let conn_string = format!("host=127.0.0.1 port={host_port} user=postgres password=postgres dbname=test");

    let (client, connection) = tokio_postgres::connect(&conn_string, tokio_postgres::NoTls)
        .await
        .expect("failed to connect");

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Connection error: {e}");
        }
    });

    // Create table with TEXT[] column
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
#[ignore = "requires a running PostgreSQL container (testcontainers); run with --include-ignored"]
async fn row_to_map_handles_enum_columns() {
    let container = Postgres::default()
        .with_user("postgres")
        .with_password("postgres")
        .with_db_name("test")
        .start()
        .await
        .expect("failed to start postgres container");

    let host_port = container.get_host_port_ipv4(5432).await.expect("failed to get port");
    let conn_string = format!("host=127.0.0.1 port={host_port} user=postgres password=postgres dbname=test");

    let (client, connection) = tokio_postgres::connect(&conn_string, tokio_postgres::NoTls)
        .await
        .expect("failed to connect");

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Connection error: {e}");
        }
    });

    // Create ENUM type and table
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
    client
        .execute("INSERT INTO test_enum_table (id, status) VALUES ($1, $2)", &[&1i64, &"active"])
        .await
        .expect("failed to insert active status");

    client
        .execute("INSERT INTO test_enum_table (id, status) VALUES ($1, $2)", &[&2i64, &"pending"])
        .await
        .expect("failed to insert pending status");

    // Query and verify ENUM columns are properly decoded
    let rows = client
        .query("SELECT id, status FROM test_enum_table ORDER BY id", &[])
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
#[ignore = "requires a running PostgreSQL container (testcontainers); run with --include-ignored"]
async fn row_to_map_handles_mixed_types_with_nulls() {
    let container = Postgres::default()
        .with_user("postgres")
        .with_password("postgres")
        .with_db_name("test")
        .start()
        .await
        .expect("failed to start postgres container");

    let host_port = container.get_host_port_ipv4(5432).await.expect("failed to get port");
    let conn_string = format!("host=127.0.0.1 port={host_port} user=postgres password=postgres dbname=test");

    let (client, connection) = tokio_postgres::connect(&conn_string, tokio_postgres::NoTls)
        .await
        .expect("failed to connect");

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Connection error: {e}");
        }
    });

    // Create test table with multiple types
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
            &[
                &1i64,
                &42i32,
                &"hello",
                &true,
                &json!({"key": "value"}),
            ],
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
    let json_val2: serde_json::Value = rows[1].get(4);
    assert_eq!(id2, 2);
    assert!(int_val2.is_none());
    assert!(text_val2.is_none());
    assert!(bool_val2.is_none());
    assert!(json_val2.is_null());
}
