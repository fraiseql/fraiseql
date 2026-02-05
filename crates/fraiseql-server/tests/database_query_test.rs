//! Database Query Integration Tests
//!
//! Tests actual database query execution:
//! 1. Connection pool initialization
//! 2. Query execution
//! 3. Result verification
//! 4. Error handling
//!
//! These tests require a running PostgreSQL instance.
//! Run with: `cargo test -p fraiseql-server --test database_query_test -- --ignored`

use std::time::Instant;

use sqlx::postgres::PgPool;

/// Helper to get database URL from environment.
/// Panics if DATABASE_URL is not set (tests using this are `#[ignore]`).
fn require_database_url() -> String {
    std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test")
}

/// Test database connection
#[tokio::test]
#[ignore = "Requires PostgreSQL: set DATABASE_URL"]
async fn test_database_connection() {
    let database_url = require_database_url();

    let pool = PgPool::connect(&database_url).await.expect("Failed to connect to database");

    let value = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&pool)
        .await
        .expect("SELECT 1 should succeed");

    assert_eq!(value, 1);

    pool.close().await;
}

/// Test connection pool configuration
#[tokio::test]
#[ignore = "Requires PostgreSQL: set DATABASE_URL"]
async fn test_connection_pool_config() {
    let database_url = require_database_url();

    let pool = PgPool::connect(&database_url).await.expect("Failed to create pool");

    // Verify pool has at least one idle connection after connect
    // (sqlx creates an initial connection on connect)
    let num_idle = pool.num_idle();
    assert!(num_idle >= 1, "pool should have at least 1 idle connection, got {num_idle}");

    pool.close().await;
}

/// Test concurrent database queries
#[tokio::test]
#[ignore = "Requires PostgreSQL: set DATABASE_URL"]
async fn test_concurrent_database_queries() {
    let database_url = require_database_url();

    let pool = PgPool::connect(&database_url).await.expect("Failed to connect");

    let futures: Vec<_> = (0..10)
        .map(|i| {
            let pool = pool.clone();
            async move { sqlx::query_scalar::<_, i32>("SELECT $1").bind(i).fetch_one(&pool).await }
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    assert_eq!(results.len(), 10);
    for (i, result) in results.iter().enumerate() {
        let value = result.as_ref().unwrap_or_else(|e| panic!("Query {i} failed: {e}"));
        assert_eq!(*value, i as i32, "Query {i} returned wrong value");
    }

    pool.close().await;
}

/// Test query performance baseline
#[tokio::test]
#[ignore = "Requires PostgreSQL: set DATABASE_URL"]
async fn test_query_performance() {
    let database_url = require_database_url();

    let pool = PgPool::connect(&database_url).await.expect("Failed to connect");

    let start = Instant::now();

    for i in 0..100 {
        let value = sqlx::query_scalar::<_, i32>("SELECT $1")
            .bind(i)
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|e| panic!("Query {i} failed: {e}"));
        assert_eq!(value, i, "Query {i} returned wrong value");
    }

    let duration = start.elapsed();

    // 100 simple queries should complete in under 5 seconds on any reasonable setup
    assert!(
        duration.as_millis() < 5000,
        "100 queries took {}ms, expected <5000ms",
        duration.as_millis()
    );

    pool.close().await;
}

/// Test connection pool under stress
#[tokio::test]
#[ignore = "Requires PostgreSQL: set DATABASE_URL"]
async fn test_connection_pool_stress() {
    let database_url = require_database_url();

    let pool = PgPool::connect(&database_url).await.expect("Failed to connect");

    let futures: Vec<_> = (0..50)
        .map(|i| {
            let pool = pool.clone();
            async move {
                sqlx::query_scalar::<_, i32>("SELECT $1 as value")
                    .bind(i)
                    .fetch_one(&pool)
                    .await
            }
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    let failures: Vec<_> = results
        .iter()
        .enumerate()
        .filter_map(|(i, r)| r.as_ref().err().map(|e| format!("query {i}: {e}")))
        .collect();

    assert!(
        failures.is_empty(),
        "all 50 concurrent queries should succeed, failures: {failures:?}"
    );

    pool.close().await;
}

/// Test transaction handling
#[tokio::test]
#[ignore = "Requires PostgreSQL: set DATABASE_URL"]
async fn test_transaction_handling() {
    let database_url = require_database_url();

    let pool = PgPool::connect(&database_url).await.expect("Failed to connect");

    let mut tx = pool.begin().await.expect("Failed to begin transaction");

    let value = sqlx::query_scalar::<_, i32>("SELECT 42")
        .fetch_one(&mut *tx)
        .await
        .expect("SELECT within transaction should succeed");

    assert_eq!(value, 42);

    tx.rollback().await.expect("Failed to rollback");

    pool.close().await;
}

/// Test error handling for nonexistent table
#[tokio::test]
#[ignore = "Requires PostgreSQL: set DATABASE_URL"]
async fn test_database_error_handling() {
    let database_url = require_database_url();

    let pool = PgPool::connect(&database_url).await.expect("Failed to connect");

    let result = sqlx::query_scalar::<_, i32>("SELECT * FROM nonexistent_table_xyz_12345")
        .fetch_one(&pool)
        .await;

    let err = result.expect_err("querying nonexistent table should fail");
    let err_str = err.to_string();
    assert!(
        err_str.contains("nonexistent_table_xyz_12345"),
        "error should mention the table name, got: {err_str}"
    );

    pool.close().await;
}

/// Test connection timeout handling
#[tokio::test]
async fn test_connection_timeout() {
    let invalid_url = "postgresql://invalid.host.example.com/db";

    let result =
        tokio::time::timeout(std::time::Duration::from_secs(2), PgPool::connect(invalid_url)).await;

    match result {
        Err(_elapsed) => {
            // Timeout -- expected behavior
        },
        Ok(Err(_connect_err)) => {
            // Connection error (DNS failure, refused) -- also acceptable
        },
        Ok(Ok(_pool)) => {
            panic!("should not successfully connect to invalid host");
        },
    }
}

/// Test prepared statements caching
#[tokio::test]
#[ignore = "Requires PostgreSQL: set DATABASE_URL"]
async fn test_prepared_statement_caching() {
    let database_url = require_database_url();

    let pool = PgPool::connect(&database_url).await.expect("Failed to connect");

    let start = Instant::now();

    // Execute same query 50 times -- should benefit from statement caching
    for i in 0..50 {
        let value = sqlx::query_scalar::<_, i32>("SELECT $1")
            .bind(i)
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|e| panic!("Query {i} failed: {e}"));
        assert_eq!(value, i);
    }

    let duration = start.elapsed();

    assert!(
        duration.as_millis() < 3000,
        "50 cached queries took {}ms, expected <3000ms",
        duration.as_millis()
    );

    pool.close().await;
}

/// Test concurrent transaction handling
#[tokio::test]
#[ignore = "Requires PostgreSQL: set DATABASE_URL"]
async fn test_concurrent_transactions() {
    let database_url = require_database_url();

    let pool = PgPool::connect(&database_url).await.expect("Failed to connect");

    let futures: Vec<_> = (0..10)
        .map(|i| {
            let pool = pool.clone();
            async move {
                let mut tx = pool.begin().await?;
                let value: i32 =
                    sqlx::query_scalar("SELECT $1").bind(i).fetch_one(&mut *tx).await?;
                tx.commit().await?;
                Ok::<i32, sqlx::Error>(value)
            }
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    for (i, result) in results.iter().enumerate() {
        let value = result.as_ref().unwrap_or_else(|e| panic!("Transaction {i} failed: {e}"));
        assert_eq!(*value, i as i32, "Transaction {i} returned wrong value");
    }

    pool.close().await;
}

/// Test pool reports valid state after queries
#[tokio::test]
#[ignore = "Requires PostgreSQL: set DATABASE_URL"]
async fn test_pool_size_limits() {
    let database_url = require_database_url();

    let pool = PgPool::connect(&database_url).await.expect("Failed to connect");

    // Run a query to ensure pool is warmed up
    let _: i32 = sqlx::query_scalar("SELECT 1").fetch_one(&pool).await.unwrap();

    let num_idle = pool.num_idle();
    assert!(num_idle >= 1, "pool should have idle connections after query, got {num_idle}");

    pool.close().await;
}
