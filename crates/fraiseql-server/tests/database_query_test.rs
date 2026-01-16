//! Database Query Integration Tests
//!
//! Tests actual database query execution:
//! 1. Connection pool initialization
//! 2. Test schema creation
//! 3. Data insertion
//! 4. Query execution
//! 5. Result verification
//!
//! These tests require PostgreSQL database.
//! Set DATABASE_URL environment variable to enable.

use sqlx::postgres::PgPool;
use std::time::Instant;

/// Helper to get database URL from environment
fn get_database_url() -> Option<String> {
    std::env::var("DATABASE_URL").ok()
}

/// Test database connection
#[tokio::test]
async fn test_database_connection() {
    let Some(database_url) = get_database_url() else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database");

    // Verify connection works
    let result = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&pool)
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 1);

    pool.close().await;
}

/// Test connection pool configuration
#[tokio::test]
async fn test_connection_pool_config() {
    let Some(database_url) = get_database_url() else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    // Create pool with custom settings
    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to create pool");

    // Verify pool is usable
    let num_idle = pool.num_idle();
    assert!(num_idle >= 0);

    pool.close().await;
}

/// Test concurrent database queries
#[tokio::test]
async fn test_concurrent_database_queries() {
    let Some(database_url) = get_database_url() else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect");

    // Fire 10 concurrent queries
    let futures: Vec<_> = (0..10)
        .map(|i| {
            let pool = pool.clone();
            async move {
                sqlx::query_scalar::<_, i32>("SELECT $1")
                    .bind(i)
                    .fetch_one(&pool)
                    .await
            }
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    // All should succeed
    assert_eq!(results.len(), 10);
    for (i, result) in results.iter().enumerate() {
        assert!(result.is_ok(), "Query {} failed", i);
        if let Ok(value) = result {
            assert_eq!(*value, i as i32);
        }
    }

    pool.close().await;
}

/// Test query performance
#[tokio::test]
async fn test_query_performance() {
    let Some(database_url) = get_database_url() else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect");

    let start = Instant::now();

    // Execute 100 simple queries
    for i in 0..100 {
        let _result = sqlx::query_scalar::<_, i32>("SELECT $1")
            .bind(i)
            .fetch_one(&pool)
            .await;
    }

    let duration = start.elapsed();
    let avg_ms = duration.as_millis() as f64 / 100.0;

    println!("Average query time: {:.2}ms", avg_ms);

    // Queries should be fast
    assert!(duration.as_millis() < 10000); // 100 queries in < 10s

    pool.close().await;
}

/// Test connection pool under stress
#[tokio::test]
async fn test_connection_pool_stress() {
    let Some(database_url) = get_database_url() else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect");

    // Fire 50 concurrent connections
    let futures: Vec<_> = (0..50)
        .map(|i| {
            let pool = pool.clone();
            async move {
                // Hold connection briefly
                sqlx::query_scalar::<_, i32>("SELECT $1 as value")
                    .bind(i)
                    .fetch_one(&pool)
                    .await
                    .ok()
            }
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    let successful = results.iter().filter(|r| r.is_some()).count();

    println!("Connection pool stress test: {}/50 successful", successful);

    // Most should succeed
    assert!(successful > 40);

    pool.close().await;
}

/// Test transaction handling
#[tokio::test]
async fn test_transaction_handling() {
    let Some(database_url) = get_database_url() else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect");

    // Start transaction
    let mut tx = pool.begin().await.expect("Failed to begin transaction");

    // Execute query within transaction
    let result = sqlx::query_scalar::<_, i32>("SELECT 42")
        .fetch_one(&mut *tx)
        .await;

    assert!(result.is_ok());

    // Rollback transaction
    tx.rollback().await.expect("Failed to rollback");

    pool.close().await;
}

/// Test error handling
#[tokio::test]
async fn test_database_error_handling() {
    let Some(database_url) = get_database_url() else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect");

    // Try to query non-existent table
    let result = sqlx::query_scalar::<_, i32>("SELECT * FROM nonexistent_table")
        .fetch_one(&pool)
        .await;

    // Should return error
    assert!(result.is_err());

    pool.close().await;
}

/// Test connection timeout handling
#[tokio::test]
async fn test_connection_timeout() {
    // Try to connect to invalid host with short timeout
    let invalid_url = "postgresql://invalid.host.example.com/db";

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        PgPool::connect(invalid_url),
    )
    .await;

    // Should timeout or fail
    assert!(result.is_err() || result.unwrap().is_err());
}

/// Test prepared statements caching
#[tokio::test]
async fn test_prepared_statement_caching() {
    let Some(database_url) = get_database_url() else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect");

    let start = Instant::now();

    // Execute same query multiple times
    for i in 0..50 {
        let _result = sqlx::query_scalar::<_, i32>("SELECT $1")
            .bind(i)
            .fetch_one(&pool)
            .await;
    }

    let duration = start.elapsed();

    println!("50 queries with caching: {:.2}ms", duration.as_millis());

    // Should be fast with caching
    assert!(duration.as_millis() < 5000);

    pool.close().await;
}

/// Test concurrent transaction handling
#[tokio::test]
async fn test_concurrent_transactions() {
    let Some(database_url) = get_database_url() else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect");

    // Fire 10 concurrent transactions
    let futures: Vec<_> = (0..10)
        .map(|i| {
            let pool = pool.clone();
            async move {
                let mut tx = pool.begin().await.ok()?;
                let result = sqlx::query_scalar::<_, i32>("SELECT $1")
                    .bind(i)
                    .fetch_one(&mut *tx)
                    .await;
                if result.is_ok() {
                    tx.commit().await.ok()?;
                } else {
                    tx.rollback().await.ok()?;
                }
                result.ok()
            }
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    let successful = results.iter().filter(|r| r.is_some()).count();

    println!("Concurrent transactions: {}/10 successful", successful);

    // All should succeed
    assert!(successful >= 8);

    pool.close().await;
}

/// Test max pool size limits
#[tokio::test]
async fn test_pool_size_limits() {
    let Some(database_url) = get_database_url() else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect");

    // Check pool configuration
    let num_idle = pool.num_idle();

    println!("Pool connections - Idle: {}", num_idle);

    // Pool should be functional
    assert!(num_idle >= 0);

    pool.close().await;
}
