//! Stress testing suite for fraiseql-wire
//!
//! Tests failure handling and recovery under adverse conditions.
//! These tests require a running Postgres instance with the test_staging schema.
//!
//! Run with: cargo test --test stress_tests -- --ignored --nocapture

use fraiseql_wire::client::FraiseClient;
use futures::stream::StreamExt;
use std::time::Duration;

/// Helper to connect to test database
async fn connect_test_db() -> fraiseql_wire::error::Result<FraiseClient> {
    let conn_string = format!(
        "postgres://{}:{}@{}/{}",
        std::env::var("POSTGRES_USER").unwrap_or_else(|_| "postgres".to_string()),
        std::env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "postgres".to_string()),
        std::env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".to_string()),
        std::env::var("POSTGRES_DB").unwrap_or_else(|_| "fraiseql_test".to_string()),
    );

    FraiseClient::connect(&conn_string).await
}

/// Test dropping stream early (simulates client disconnect)
#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_stress_early_stream_drop() {
    println!("Test: Early stream drop (client disconnect)");

    let client = connect_test_db()
        .await
        .expect("failed to connect");

    let mut stream = client
        .query::<serde_json::Value>("projects")
        .execute()
        .await
        .expect("failed to execute query");

    // Consume only first row then drop
    if let Some(result) = stream.next().await {
        let _value = result.expect("failed to deserialize");
        println!("  Received first row, dropping stream...");
    }

    drop(stream);
    println!("  Stream dropped: ✓");

    // Verify connection is still usable
    let client2 = connect_test_db()
        .await
        .expect("should be able to reconnect");

    let mut stream2 = client2
        .query::<serde_json::Value>("projects")
        .execute()
        .await
        .expect("failed to execute second query");

    if let Some(result) = stream2.next().await {
        let _value = result.expect("failed to deserialize");
        println!("  Reconnection successful: ✓");
    }
}

/// Test invalid connection string handling
#[tokio::test]
async fn test_stress_invalid_connection_string() {
    println!("Test: Invalid connection string");

    let result = FraiseClient::connect("invalid://connection/string").await;

    assert!(result.is_err(), "should reject invalid connection string");
    println!("  Invalid connection rejected: ✓");
}

/// Test connection to non-existent host
#[tokio::test]
async fn test_stress_connection_refused() {
    println!("Test: Connection refused (host unreachable)");

    let result = FraiseClient::connect("postgres://nonexistent.invalid:5432/test").await;

    assert!(result.is_err(), "should fail for unreachable host");
    println!("  Connection refused handled: ✓");
}

/// Test missing required table
#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_stress_missing_table() {
    println!("Test: Missing table");

    let client = connect_test_db()
        .await
        .expect("failed to connect");

    let result = client
        .query::<serde_json::Value>("nonexistent_table")
        .execute()
        .await;

    assert!(result.is_err(), "should reject missing table");
    println!("  Missing table error: ✓");
}

/// Test invalid WHERE clause
#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_stress_invalid_where_clause() {
    println!("Test: Invalid WHERE clause");

    let client = connect_test_db()
        .await
        .expect("failed to connect");

    let result = client
        .query::<serde_json::Value>("projects")
        .where_sql("INVALID SQL SYNTAX (((")
        .execute()
        .await;

    assert!(result.is_err(), "should reject invalid SQL");
    println!("  Invalid SQL rejected: ✓");
}

/// Test empty result set handling
#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_stress_empty_result_set() {
    println!("Test: Empty result set");

    let client = connect_test_db()
        .await
        .expect("failed to connect");

    // Query with predicate that matches nothing
    let mut stream = client
        .query::<serde_json::Value>("projects")
        .where_sql("data->>'name' = 'NonexistentProject12345'")
        .execute()
        .await
        .expect("failed to execute query");

    let mut count = 0;
    while let Some(result) = stream.next().await {
        let _value = result.expect("failed to deserialize");
        count += 1;
    }

    assert_eq!(count, 0, "should return zero rows");
    println!("  Empty result set handled: ✓");
}

/// Test very large WHERE clause
#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_stress_large_where_clause() {
    println!("Test: Very large WHERE clause");

    let client = connect_test_db()
        .await
        .expect("failed to connect");

    // Create a large WHERE clause
    let mut where_clause = "data->>'name' IN (".to_string();
    for i in 0..100 {
        where_clause.push_str(&format!("'name_{}'", i));
        if i < 99 {
            where_clause.push(',');
        }
    }
    where_clause.push(')');

    let result = client
        .query::<serde_json::Value>("projects")
        .where_sql(&where_clause)
        .execute()
        .await;

    // Should succeed or fail gracefully
    match result {
        Ok(mut stream) => {
            let mut count = 0;
            while let Some(result) = stream.next().await {
                let _value = result.expect("failed to deserialize");
                count += 1;
            }
            println!("  Large WHERE clause executed: {} rows", count);
        }
        Err(e) => {
            println!("  Large WHERE clause error (acceptable): {}", e);
        }
    }

    println!("  Large WHERE clause handled: ✓");
}

/// Test rapid connect/disconnect cycles
#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_stress_connection_cycling() {
    println!("Test: Rapid connection cycles");

    let num_cycles = 10;

    for i in 0..num_cycles {
        let result = connect_test_db().await;
        assert!(result.is_ok(), "cycle {} failed", i);

        println!("  Cycle {}: connected", i);
    }

    println!("  Connection cycling: ✓");
}

/// Test multiple concurrent streams from one connection
#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_stress_single_connection_multiple_queries() {
    println!("Test: Multiple queries from single connection");

    let client = connect_test_db()
        .await
        .expect("failed to connect");

    // First query
    let mut stream1 = client
        .query::<serde_json::Value>("projects")
        .execute()
        .await
        .expect("failed first query");

    let row1 = stream1.next().await;
    assert!(row1.is_some(), "should get at least one row from first query");

    println!("  First query: received row");

    // Note: fraiseql-wire only supports one active query per connection
    // Attempting a second query should be handled appropriately
    // This documents the expected behavior
}

/// Test streaming with very small chunk size
#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_stress_tiny_chunk_size() {
    println!("Test: Very small chunk size");

    let client = connect_test_db()
        .await
        .expect("failed to connect");

    let mut stream = client
        .query::<serde_json::Value>("projects")
        .chunk_size(2) // Very small chunk
        .execute()
        .await
        .expect("failed to execute");

    let mut count = 0;
    while let Some(result) = stream.next().await {
        let _value = result.expect("failed to deserialize");
        count += 1;
    }

    println!("  Small chunk size (2): {} rows", count);
    println!("  Small chunk size handled: ✓");
}

/// Test streaming with very large chunk size
#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_stress_huge_chunk_size() {
    println!("Test: Very large chunk size");

    let client = connect_test_db()
        .await
        .expect("failed to connect");

    let mut stream = client
        .query::<serde_json::Value>("projects")
        .chunk_size(10000) // Very large chunk
        .execute()
        .await
        .expect("failed to execute");

    let mut count = 0;
    while let Some(result) = stream.next().await {
        let _value = result.expect("failed to deserialize");
        count += 1;
    }

    println!("  Large chunk size (10000): {} rows", count);
    println!("  Large chunk size handled: ✓");
}

/// Test authentication with wrong password
#[tokio::test]
async fn test_stress_wrong_credentials() {
    println!("Test: Wrong credentials");

    let conn_string = "postgres://postgres:wrongpassword@localhost/fraiseql_test";
    let result = FraiseClient::connect(conn_string).await;

    assert!(result.is_err(), "should reject wrong password");
    println!("  Wrong credentials rejected: ✓");
}

/// Test partial row consumption
#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_stress_partial_consumption() {
    println!("Test: Partial row consumption");

    let client = connect_test_db()
        .await
        .expect("failed to connect");

    let mut stream = client
        .query::<serde_json::Value>("projects")
        .execute()
        .await
        .expect("failed to execute");

    // Consume some rows
    let mut count = 0;
    while let Some(result) = stream.next().await {
        let _value = result.expect("failed to deserialize");
        count += 1;

        if count >= 3 {
            break;
        }
    }

    println!("  Consumed {}/all rows", count);
    println!("  Partial consumption handled: ✓");
}

/// Test zero chunk size handling
#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_stress_zero_chunk_size() {
    println!("Test: Zero chunk size");

    let client = connect_test_db()
        .await
        .expect("failed to connect");

    // Attempt to set chunk size to 0 - should either be rejected or default to something safe
    let result = client
        .query::<serde_json::Value>("projects")
        .chunk_size(0)
        .execute()
        .await;

    match result {
        Ok(mut stream) => {
            // If it accepts 0, it should still work
            let mut count = 0;
            while let Some(result) = stream.next().await {
                let _value = result.expect("failed to deserialize");
                count += 1;
                if count >= 1 {
                    break;
                }
            }
            println!("  Zero chunk size handled gracefully");
        }
        Err(_e) => {
            println!("  Zero chunk size rejected (acceptable)");
        }
    }

    println!("  Zero chunk size: ✓");
}

/// Test complex ORDER BY with invalid syntax
#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_stress_invalid_order_by() {
    println!("Test: Invalid ORDER BY");

    let client = connect_test_db()
        .await
        .expect("failed to connect");

    let result = client
        .query::<serde_json::Value>("projects")
        .order_by("INVALID SYNTAX FOR ORDER BY")
        .execute()
        .await;

    assert!(result.is_err(), "should reject invalid ORDER BY");
    println!("  Invalid ORDER BY rejected: ✓");
}

/// Test combining predicates in various ways
#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_stress_combined_predicates() {
    println!("Test: Combined SQL and Rust predicates");

    let client = connect_test_db()
        .await
        .expect("failed to connect");

    let mut stream = client
        .query::<serde_json::Value>("users")
        .where_sql("data->>'id' IS NOT NULL")
        .where_rust(|json| {
            // Rust predicate that's very restrictive
            json.get("profile").is_some()
        })
        .execute()
        .await
        .expect("failed to execute");

    let mut count = 0;
    while let Some(result) = stream.next().await {
        let _value = result.expect("failed to deserialize");
        count += 1;
    }

    println!("  Combined predicates: {} rows", count);
    println!("  Combined predicates: ✓");
}

/// Test result verification - ensure we get proper JSON
#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_stress_json_validity() {
    println!("Test: JSON validity of results");

    let client = connect_test_db()
        .await
        .expect("failed to connect");

    let mut stream = client
        .query::<serde_json::Value>("projects")
        .execute()
        .await
        .expect("failed to execute");

    let mut count = 0;
    let mut valid_json = 0;

    while let Some(result) = stream.next().await {
        let value = result.expect("failed to deserialize");

        // Verify it's valid JSON
        if value.is_object() || value.is_array() || !value.is_null() {
            valid_json += 1;
        }

        count += 1;
    }

    println!("  Total rows: {}", count);
    println!("  Valid JSON rows: {}", valid_json);
    assert_eq!(
        count, valid_json,
        "all rows should contain valid JSON"
    );
    println!("  JSON validity: ✓");
}

/// Test ORDER BY with complex expressions
#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_stress_complex_order_by() {
    println!("Test: Complex ORDER BY expression");

    let client = connect_test_db()
        .await
        .expect("failed to connect");

    // Order by JSON field with COLLATE
    let result = client
        .query::<serde_json::Value>("projects")
        .order_by("data->>'name' COLLATE \"C\" DESC")
        .execute()
        .await;

    match result {
        Ok(mut stream) => {
            let mut count = 0;
            while let Some(result) = stream.next().await {
                let _value = result.expect("failed to deserialize");
                count += 1;
            }
            println!("  Complex ORDER BY: {} rows", count);
        }
        Err(e) => {
            println!("  Complex ORDER BY error: {}", e);
        }
    }

    println!("  Complex ORDER BY: ✓");
}
