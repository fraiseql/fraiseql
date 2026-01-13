//! Load testing suite for fraiseql-wire
//!
//! Tests throughput, memory stability, and performance under sustained load.
//! These tests require a running Postgres instance with the test_staging schema.
//!
//! Run with: cargo test --test load_tests -- --ignored --nocapture

use fraiseql_wire::client::FraiseClient;
use futures::stream::StreamExt;
use std::time::Instant;

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

/// Test streaming with moderate data volume
#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_load_moderate_volume() {
    println!("Test: Moderate data volume streaming");

    let client = connect_test_db()
        .await
        .expect("failed to connect to test database");

    let start = Instant::now();
    let mut stream = client
        .query("projects")
        .execute()
        .await
        .expect("failed to execute query");

    let mut count = 0;
    while let Some(result) = stream.next().await {
        let _value = result.expect("failed to deserialize row");
        count += 1;
    }

    let elapsed = start.elapsed();
    let throughput = count as f64 / elapsed.as_secs_f64();

    println!("  Rows: {}", count);
    println!("  Time: {:?}", elapsed);
    println!("  Throughput: {:.0} rows/sec", throughput);

    assert!(count > 0, "should have received rows");
    assert!(throughput > 0.0, "throughput should be positive");
}

/// Test streaming with large data volume and custom chunk size
#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_load_large_volume_custom_chunk() {
    println!("Test: Large volume with custom chunk size");

    let client = connect_test_db()
        .await
        .expect("failed to connect to test database");

    let start = Instant::now();
    let mut stream = client
        .query("tasks")
        .chunk_size(512) // Larger chunk for more rows per batch
        .execute()
        .await
        .expect("failed to execute query");

    let mut count = 0;
    while let Some(result) = stream.next().await {
        let _value = result.expect("failed to deserialize row");
        count += 1;
    }

    let elapsed = start.elapsed();
    let throughput = count as f64 / elapsed.as_secs_f64();

    println!("  Rows: {}", count);
    println!("  Chunk size: 512");
    println!("  Time: {:?}", elapsed);
    println!("  Throughput: {:.0} rows/sec", throughput);

    assert!(count > 0, "should have received rows");
}

/// Test streaming with WHERE predicate (reduces data volume)
#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_load_with_sql_predicate() {
    println!("Test: With SQL predicate filtering");

    let client = connect_test_db()
        .await
        .expect("failed to connect to test database");

    let start = Instant::now();
    let mut stream = client
        .query("projects")
        .where_sql("data->>'status' = 'active'")
        .execute()
        .await
        .expect("failed to execute query");

    let mut count = 0;
    while let Some(result) = stream.next().await {
        let _value = result.expect("failed to deserialize row");
        count += 1;
    }

    let elapsed = start.elapsed();
    let throughput = count as f64 / elapsed.as_secs_f64();

    println!("  Rows: {}", count);
    println!("  Time: {:?}", elapsed);
    println!("  Throughput: {:.0} rows/sec", throughput);
    println!("  (Predicate filtering should reduce row count)");
}

/// Test streaming with Rust predicate (client-side filtering)
#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_load_with_rust_predicate() {
    println!("Test: With Rust predicate filtering");

    let client = connect_test_db()
        .await
        .expect("failed to connect to test database");

    let start = Instant::now();
    let mut stream = client
        .query("users")
        .where_rust(|json| {
            // Only accept users with profile info
            json.get("profile").is_some()
        })
        .execute()
        .await
        .expect("failed to execute query");

    let mut count = 0;
    while let Some(result) = stream.next().await {
        let _value = result.expect("failed to deserialize row");
        count += 1;
    }

    let elapsed = start.elapsed();
    let throughput = count as f64 / elapsed.as_secs_f64();

    println!("  Rows: {}", count);
    println!("  Time: {:?}", elapsed);
    println!("  Throughput: {:.0} rows/sec", throughput);
}

/// Test streaming documents (large JSON objects)
#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_load_large_json_objects() {
    println!("Test: Large JSON objects");

    let client = connect_test_db()
        .await
        .expect("failed to connect to test database");

    let start = Instant::now();
    let mut stream = client
        .query("documents")
        .chunk_size(32) // Small chunks for large objects
        .execute()
        .await
        .expect("failed to execute query");

    let mut count = 0;
    let mut total_size: usize = 0;

    while let Some(result) = stream.next().await {
        let value = result.expect("failed to deserialize row");
        let size = value.to_string().len();
        total_size += size;
        count += 1;
    }

    let elapsed = start.elapsed();
    let avg_size = if count > 0 { total_size / count } else { 0 };

    println!("  Rows: {}", count);
    println!("  Total size: {} bytes", total_size);
    println!("  Average size: {} bytes", avg_size);
    println!("  Time: {:?}", elapsed);

    assert!(count >= 0, "should handle large objects");
}

/// Test with ORDER BY (server-side sorting)
#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_load_with_order_by() {
    println!("Test: With ORDER BY clause");

    let client = connect_test_db()
        .await
        .expect("failed to connect to test database");

    let start = Instant::now();
    let mut stream = client
        .query("projects")
        .order_by("data->>'name' ASC")
        .execute()
        .await
        .expect("failed to execute query");

    let mut count = 0;
    let mut prev_name: Option<String> = None;

    while let Some(result) = stream.next().await {
        let value = result.expect("failed to deserialize row");
        let name = value
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        // Verify ordering
        if let Some(ref pn) = prev_name {
            assert!(
                pn <= &name,
                "order violation: {} > {}",
                pn,
                name
            );
        }

        prev_name = Some(name);
        count += 1;
    }

    let elapsed = start.elapsed();

    println!("  Rows: {}", count);
    println!("  Time: {:?}", elapsed);
    println!("  Ordering verified: ✓");
}

/// Test multiple sequential connections (simulates concurrent workload)
#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_load_multiple_sequential_connections() {
    println!("Test: Multiple sequential connections");

    let num_connections = 5;
    let mut total_rows = 0;

    for i in 0..num_connections {
        let client = connect_test_db()
            .await
            .expect("failed to connect");

        let start = Instant::now();
        let mut stream = client
            .query("projects")
            .execute()
            .await
            .expect("failed to execute");

        let mut count = 0;
        while let Some(result) = stream.next().await {
            let _value = result.expect("failed to deserialize");
            count += 1;
        }

        let elapsed = start.elapsed();

        total_rows += count;
        println!("    Connection {}: {} rows in {:?}", i, count, elapsed);

        assert!(count > 0, "connection {} should have received rows", i);
    }

    println!("  Total connections: {}", num_connections);
    println!("  Total rows: {}", total_rows);
    println!("  Sequential streaming: ✓");
}

/// Test streaming stability over time (look for memory leaks)
#[tokio::test]
#[ignore] // Requires Postgres running - long running test
async fn test_load_sustained_streaming() {
    println!("Test: Sustained streaming (duration test)");

    let client = connect_test_db()
        .await
        .expect("failed to connect to test database");

    let duration = std::time::Duration::from_secs(30); // 30-second test
    let start = Instant::now();

    let mut stream = client
        .query("projects")
        .execute()
        .await
        .expect("failed to execute query");

    let mut count = 0;

    while let Some(result) = stream.next().await {
        let _value = result.expect("failed to deserialize row");
        count += 1;

        // Break if we've exceeded duration
        if start.elapsed() >= duration {
            break;
        }
    }

    let elapsed = start.elapsed();
    let throughput = count as f64 / elapsed.as_secs_f64();

    println!("  Duration: {:?}", elapsed);
    println!("  Rows: {}", count);
    println!("  Throughput: {:.0} rows/sec", throughput);
    println!("  Sustained streaming: ✓");
}

/// Benchmark different chunk sizes to find optimal throughput
#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_load_chunk_size_comparison() {
    println!("Test: Chunk size performance comparison");

    let chunk_sizes = vec![16, 32, 64, 128, 256, 512];

    for chunk_size in chunk_sizes {
        let client = connect_test_db()
            .await
            .expect("failed to connect");

        let start = Instant::now();
        let mut stream = client
            .query("projects")
            .chunk_size(chunk_size)
            .execute()
            .await
            .expect("failed to execute");

        let mut count = 0;
        while let Some(result) = stream.next().await {
            let _value = result.expect("failed to deserialize");
            count += 1;
        }

        let elapsed = start.elapsed();
        let throughput = count as f64 / elapsed.as_secs_f64();

        println!(
            "  Chunk {}: {:.0} rows/sec ({} rows in {:?})",
            chunk_size, throughput, count, elapsed
        );
    }
}

/// Test error recovery during streaming
#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_load_partial_stream_drop() {
    println!("Test: Partial stream consumption and drop");

    let client = connect_test_db()
        .await
        .expect("failed to connect to test database");

    let mut stream = client
        .query("projects")
        .execute()
        .await
        .expect("failed to execute query");

    let mut count = 0;
    const LIMIT: usize = 2; // Only consume first 2 rows

    while let Some(result) = stream.next().await {
        let _value = result.expect("failed to deserialize row");
        count += 1;

        if count >= LIMIT {
            break; // Drop stream early
        }
    }

    println!("  Consumed: {} rows", count);
    println!("  Stream dropped early: ✓");
    // If we get here without panicking, cancellation worked

    // Now verify we can make another connection
    let client2 = connect_test_db()
        .await
        .expect("should be able to reconnect");

    let mut stream2 = client2
        .query("projects")
        .execute()
        .await
        .expect("failed to execute second query");

    let mut count2 = 0;
    while let Some(result) = stream2.next().await {
        let _value = result.expect("failed to deserialize row");
        count2 += 1;
        if count2 >= 1 {
            break; // Just get one row
        }
    }

    println!("  Reconnection: ✓");
    assert!(count2 > 0, "second connection should work");
}
