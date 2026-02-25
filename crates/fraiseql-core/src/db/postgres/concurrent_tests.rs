//! Concurrent nested query tests for Issue #41
//!
//! Tests that verify complex nested queries (Q3: comments with author+post)
//! work correctly under concurrent load without connection pool exhaustion.

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
        time::{Duration, Instant},
    };

    use fraiseql_core::{
        db::{DatabaseAdapter, postgres::PostgresAdapter},
        error::FraiseQLError,
        runtime::{Executor, RuntimeConfig},
        schema::CompiledSchema,
    };

    /// Test configuration
    const TEST_DURATION_SECS: u64 = 5; // Short duration for unit tests
    const CONCURRENT_WORKERS: &[usize] = &[1, 5, 10, 20];

    /// Q3 query: comments with nested author + post
    const Q3_QUERY: &str =
        r#"{ comments(limit: 20) { id content author { username } post { title } } }"#;

    /// Simple query for comparison
    const Q1_QUERY: &str = r#"{ users(limit: 10) { id name } }"#;

    /// Run concurrent load test
    async fn run_concurrent_test(
        executor: Arc<Executor<PostgresAdapter>>,
        query: &str,
        num_workers: usize,
        duration_secs: u64,
    ) -> ConcurrentTestResult {
        let success_count = Arc::new(AtomicU64::new(0));
        let failure_count = Arc::new(AtomicU64::new(0));
        let start_time = Instant::now();

        let mut handles = vec![];
        for worker_id in 0..num_workers {
            let executor = Arc::clone(&executor);
            let success = Arc::clone(&success_count);
            let failure = Arc::clone(&failure_count);
            let query = query.to_string();

            let handle = tokio::spawn(async move {
                let worker_start = Instant::now();
                let mut local_success = 0u64;
                let mut local_failure = 0u64;

                while worker_start.elapsed() < Duration::from_secs(duration_secs) {
                    match executor.execute(&query, None).await {
                        Ok(_) => local_success += 1,
                        Err(e) => {
                            local_failure += 1;
                            if local_failure <= 3 {
                                // Log first few errors
                                eprintln!("Worker {} error: {:?}", worker_id, e);
                            }
                        },
                    }
                }

                success.fetch_add(local_success, Ordering::Relaxed);
                failure.fetch_add(local_failure, Ordering::Relaxed);
            });

            handles.push(handle);
        }

        for handle in handles {
            let _ = handle.await;
        }

        let total_elapsed = start_time.elapsed();
        let total_requests =
            success_count.load(Ordering::Relaxed) + failure_count.load(Ordering::Relaxed);
        let rps = total_requests as f64 / total_elapsed.as_secs_f64();

        ConcurrentTestResult {
            num_workers,
            total_requests,
            success_count: success_count.load(Ordering::Relaxed),
            failure_count: failure_count.load(Ordering::Relaxed),
            success_rate: success_count.load(Ordering::Relaxed) as f64
                / total_requests.max(1) as f64,
            rps,
        }
    }

    struct ConcurrentTestResult {
        num_workers:    usize,
        total_requests: u64,
        success_count:  u64,
        failure_count:  u64,
        success_rate:   f64,
        rps:            f64,
    }

    /// Test 1: Single request baseline (should always work)
    #[tokio::test]
    #[ignore = "Requires running PostgreSQL database"]
    async fn test_nested_query_single_request() {
        let adapter = PostgresAdapter::new("postgresql://localhost/testdb")
            .await
            .expect("Failed to create adapter");

        // Note: In real test, would load actual schema
        let schema = CompiledSchema::default();
        let executor = Executor::with_config(schema, Arc::new(adapter), RuntimeConfig::default());

        let result = executor.execute(Q3_QUERY, None).await;
        assert!(result.is_ok(), "Single request should succeed: {:?}", result);
    }

    /// Test 2: Concurrent nested queries should work with proper pool sizing
    ///
    /// This test verifies Issue #41 fix: nested queries no longer fail
    /// with 100% connection errors under concurrent load.
    #[tokio::test]
    #[ignore = "Requires running PostgreSQL database with test data"]
    async fn test_concurrent_nested_queries_with_proper_pool_sizing() {
        // Create adapter with the new default pool size (25)
        let adapter = PostgresAdapter::new("postgresql://localhost/testdb")
            .await
            .expect("Failed to create adapter");

        // Verify pool size is correct (25, not 10)
        let metrics = adapter.pool_metrics();
        assert!(
            metrics.total_connections >= 20,
            "Pool should have at least 20 connections (actual: {})",
            metrics.total_connections
        );

        let schema = CompiledSchema::default();
        let executor =
            Arc::new(Executor::with_config(schema, Arc::new(adapter), RuntimeConfig::default()));

        let mut results = vec![];

        for &num_workers in CONCURRENT_WORKERS {
            let result = run_concurrent_test(
                Arc::clone(&executor),
                Q3_QUERY,
                num_workers,
                TEST_DURATION_SECS,
            )
            .await;
            results.push(result);
        }

        // Verify all tests have >95% success rate
        for result in &results {
            assert!(
                result.success_rate >= 0.95,
                "Worker count {} should have >=95% success rate (actual: {:.1}%)",
                result.num_workers,
                result.success_rate * 100.0
            );
        }

        // Verify 20 workers can achieve reasonable throughput
        let twenty_worker_result = results.iter().find(|r| r.num_workers == 20).unwrap();
        assert!(
            twenty_worker_result.rps >= 100.0,
            "20 workers should achieve at least 100 RPS (actual: {:.1})",
            twenty_worker_result.rps
        );
    }

    /// Test 3: Verify connection retry logic works
    #[tokio::test]
    #[ignore = "Requires running PostgreSQL database"]
    async fn test_connection_retry_logic() {
        let adapter = PostgresAdapter::new("postgresql://localhost/testdb")
            .await
            .expect("Failed to create adapter");

        // Execute multiple queries in rapid succession
        // This tests that retry logic handles transient pool exhaustion
        let mut success_count = 0;
        let mut failure_count = 0;

        for i in 0..50 {
            let start = Instant::now();
            match adapter.health_check().await {
                Ok(_) => success_count += 1,
                Err(FraiseQLError::ConnectionPool { message }) => {
                    failure_count += 1;
                    // Verify retry happened (delay > 0)
                    assert!(
                        start.elapsed() >= Duration::from_millis(50),
                        "Retry should have added delay"
                    );
                    assert!(
                        message.contains("after 3 retries"),
                        "Error should mention retries: {}",
                        message
                    );
                },
                Err(e) => panic!("Unexpected error: {:?}", e),
            }
        }

        // Most should succeed (allowing for some failures under heavy load)
        let success_rate = success_count as f64 / 50.0;
        assert!(
            success_rate >= 0.90,
            "Health checks should have >=90% success with retries (actual: {:.1}%)",
            success_rate * 100.0
        );
    }

    /// Test 4: Pool metrics are accurate
    #[tokio::test]
    #[ignore = "Requires running PostgreSQL database"]
    async fn test_pool_metrics_accuracy() {
        let adapter = PostgresAdapter::new("postgresql://localhost/testdb")
            .await
            .expect("Failed to create adapter");

        let initial_metrics = adapter.pool_metrics();

        // Verify pool size is correct
        assert_eq!(
            initial_metrics.total_connections, 25,
            "Pool should have 25 connections (new default)"
        );

        // Initially, most connections should be idle
        assert!(
            initial_metrics.idle_connections >= 20,
            "Initially, most connections should be idle"
        );
        assert_eq!(initial_metrics.active_connections, 0, "No active connections initially");

        // Execute a query to activate a connection
        let _ = adapter.health_check().await;

        let metrics_after = adapter.pool_metrics();
        // After health check, we should have one active connection
        // (Note: deadpool returns connections to pool immediately after use)
        assert!(
            metrics_after.idle_connections >= 19,
            "Connection should be returned to pool after use"
        );
    }

    /// Test 5: Simple vs nested query performance comparison
    #[tokio::test]
    #[ignore = "Requires running PostgreSQL database with test data"]
    async fn test_simple_vs_nested_performance() {
        let adapter = PostgresAdapter::new("postgresql://localhost/testdb")
            .await
            .expect("Failed to create adapter");

        let schema = CompiledSchema::default();
        let executor =
            Arc::new(Executor::with_config(schema, Arc::new(adapter), RuntimeConfig::default()));

        // Test simple query
        let simple_result =
            run_concurrent_test(Arc::clone(&executor), Q1_QUERY, 10, TEST_DURATION_SECS).await;

        // Test nested query
        let nested_result =
            run_concurrent_test(Arc::clone(&executor), Q3_QUERY, 10, TEST_DURATION_SECS).await;

        // Both should have high success rates
        assert!(simple_result.success_rate >= 0.95, "Simple query should have >=95% success");
        assert!(
            nested_result.success_rate >= 0.95,
            "Nested query should have >=95% success (Issue #41 fix)"
        );

        // Both should achieve reasonable throughput
        assert!(simple_result.rps >= 100.0, "Simple query should have >=100 RPS");
        assert!(nested_result.rps >= 50.0, "Nested query should have >=50 RPS");

        // Nested query may be slower but should not fail
        println!(
            "Simple query: {:.1} RPS, {:.1}% success",
            simple_result.rps,
            simple_result.success_rate * 100.0
        );
        println!(
            "Nested query: {:.1} RPS, {:.1}% success",
            nested_result.rps,
            nested_result.success_rate * 100.0
        );
    }
}
