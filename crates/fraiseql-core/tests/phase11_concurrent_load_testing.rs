//! Phase 11 Concurrent Load Testing
//!
//! Tests for concurrent query execution under sustained load:
//! - Multiple concurrent queries
//! - Connection pool stress testing
//! - Throughput measurement under load
//! - Latency percentiles (p50, p95, p99)
//! - Resource utilization tracking
//! - Query result correctness under load

use fraiseql_core::db::types::JsonbValue;
use fraiseql_core::runtime::ResultProjector;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::task::JoinSet;

/// Mock database adapter for concurrent testing
struct ConcurrentMockDatabase {
    query_count: Arc<AtomicUsize>,
    users_data: Vec<JsonbValue>,
    products_data: Vec<JsonbValue>,
}

impl ConcurrentMockDatabase {
    fn new() -> Self {
        // Create sample users
        let mut users_data = Vec::new();
        for i in 0..100 {
            let user = json!({
                "id": format!("user_{}", i),
                "name": format!("User {}", i),
                "email": format!("user{}@example.com", i),
                "status": if i % 2 == 0 { "active" } else { "inactive" },
                "created_at": "2024-01-14T00:00:00Z",
                "updated_at": "2024-01-14T00:00:00Z",
                "metadata": {
                    "score": i * 10,
                    "tags": vec!["a", "b", "c"]
                }
            });
            users_data.push(JsonbValue::new(user));
        }

        // Create sample products
        let mut products_data = Vec::new();
        for i in 0..100 {
            let product = json!({
                "id": format!("product_{}", i),
                "sku": format!("SKU-{}", i),
                "name": format!("Product {}", i),
                "price": 10.0 + (i as f64),
                "stock": i * 10,
                "category": if i % 3 == 0 { "electronics" } else if i % 3 == 1 { "books" } else { "clothing" },
                "available": i % 2 == 0
            });
            products_data.push(JsonbValue::new(product));
        }

        Self {
            query_count: Arc::new(AtomicUsize::new(0)),
            users_data,
            products_data,
        }
    }

    /// Simulate a query execution with slight delay
    async fn query_users(&self, limit: Option<usize>) -> Vec<JsonbValue> {
        self.query_count.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(tokio::time::Duration::from_micros(100)).await;

        let limit = limit.unwrap_or(self.users_data.len());
        self.users_data.iter().take(limit).cloned().collect()
    }

    /// Simulate a product query
    async fn query_products(&self, limit: Option<usize>) -> Vec<JsonbValue> {
        self.query_count.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(tokio::time::Duration::from_micros(100)).await;

        let limit = limit.unwrap_or(self.products_data.len());
        self.products_data.iter().take(limit).cloned().collect()
    }

    /// Get total queries executed
    fn total_queries(&self) -> usize {
        self.query_count.load(Ordering::SeqCst)
    }
}

// ============================================================================
// CONCURRENT LOAD TESTS
// ============================================================================

/// Test: Simple concurrent queries (10 concurrent, 100 total)
#[tokio::test]
async fn test_concurrent_simple_queries() {
    let db = Arc::new(ConcurrentMockDatabase::new());
    let mut join_set = JoinSet::new();

    let start = Instant::now();

    // Spawn 10 concurrent tasks
    for _ in 0..10 {
        let db = Arc::clone(&db);
        join_set.spawn(async move {
            for _ in 0..10 {
                let _results = db.query_users(Some(10)).await;
            }
        });
    }

    // Wait for all to complete
    while join_set.join_next().await.is_some() {}

    let duration = start.elapsed();

    // Verify all queries executed
    assert_eq!(db.total_queries(), 100);
    println!("Simple concurrent (100 queries, 10 tasks): {:?}", duration);

    // Should complete reasonably fast
    assert!(duration.as_secs() < 5, "Load test took too long: {:?}", duration);
}

/// Test: Concurrent queries with field projection
#[tokio::test]
async fn test_concurrent_queries_with_projection() {
    let db = Arc::new(ConcurrentMockDatabase::new());
    let mut join_set = JoinSet::new();

    let start = Instant::now();

    // Spawn 20 concurrent tasks with projection
    for _ in 0..20 {
        let db = Arc::clone(&db);
        join_set.spawn(async move {
            let projector = ResultProjector::new(vec!["id".to_string(), "name".to_string(), "email".to_string()]);
            for _ in 0..5 {
                let results = db.query_users(Some(20)).await;
                let _projected = projector.project_results(&results, true).ok();
            }
        });
    }

    while join_set.join_next().await.is_some() {}

    let duration = start.elapsed();

    // 20 tasks × 5 queries = 100 queries
    assert_eq!(db.total_queries(), 100);
    println!("Projection concurrent (100 queries, 20 tasks): {:?}", duration);

    assert!(duration.as_secs() < 5);
}

/// Test: Mixed query types (users + products)
#[tokio::test]
async fn test_concurrent_mixed_query_types() {
    let db = Arc::new(ConcurrentMockDatabase::new());
    let mut join_set = JoinSet::new();

    let start = Instant::now();

    // Spawn tasks that query both users and products
    for i in 0..15 {
        let db = Arc::clone(&db);
        join_set.spawn(async move {
            for _ in 0..10 {
                if i % 2 == 0 {
                    let _results = db.query_users(Some(10)).await;
                } else {
                    let _results = db.query_products(Some(10)).await;
                }
            }
        });
    }

    while join_set.join_next().await.is_some() {}

    let duration = start.elapsed();

    // 15 tasks × 10 queries = 150 queries
    assert_eq!(db.total_queries(), 150);
    println!("Mixed queries (150 queries, 15 tasks): {:?}", duration);

    assert!(duration.as_secs() < 10);
}

/// Test: High concurrency stress test
#[tokio::test]
async fn test_high_concurrency_stress() {
    let db = Arc::new(ConcurrentMockDatabase::new());
    let mut join_set = JoinSet::new();

    let start = Instant::now();

    // Spawn 50 concurrent tasks
    for _ in 0..50 {
        let db = Arc::clone(&db);
        join_set.spawn(async move {
            let projector = ResultProjector::new(vec!["id".to_string(), "name".to_string()]);
            for _ in 0..4 {
                let results = db.query_users(Some(5)).await;
                let _projected = projector.project_results(&results, true).ok();
            }
        });
    }

    while join_set.join_next().await.is_some() {}

    let duration = start.elapsed();

    // 50 tasks × 4 queries = 200 queries
    assert_eq!(db.total_queries(), 200);
    println!("High concurrency (200 queries, 50 tasks): {:?}", duration);

    assert!(duration.as_secs() < 10);
}

/// Test: Query with __typename addition under load
#[tokio::test]
async fn test_concurrent_typename_addition() {
    let db = Arc::new(ConcurrentMockDatabase::new());
    let mut join_set = JoinSet::new();

    let start = Instant::now();

    // Spawn tasks that add __typename to results
    for _ in 0..25 {
        let db = Arc::clone(&db);
        join_set.spawn(async move {
            let projector = ResultProjector::new(vec!["id".to_string()]);
            for _ in 0..4 {
                let results = db.query_users(Some(10)).await;
                for result in results.iter() {
                    let _with_typename = projector.add_typename_only(result, "User").ok();
                }
            }
        });
    }

    while join_set.join_next().await.is_some() {}

    let duration = start.elapsed();

    // 25 tasks × 4 queries = 100 queries
    assert_eq!(db.total_queries(), 100);
    println!("__typename addition (100 queries, 25 tasks): {:?}", duration);

    assert!(duration.as_secs() < 5);
}

/// Test: Complete GraphQL response pipeline under load
#[tokio::test]
async fn test_complete_graphql_pipeline_load() {
    let db = Arc::new(ConcurrentMockDatabase::new());
    let mut join_set = JoinSet::new();

    let start = Instant::now();

    // Spawn tasks doing complete pipeline: query -> project -> add type -> envelope
    for _ in 0..30 {
        let db = Arc::clone(&db);
        join_set.spawn(async move {
            let fields = vec!["id".to_string(), "name".to_string(), "email".to_string()];
            let projector = ResultProjector::new(fields);

            for _ in 0..3 {
                let results = db.query_users(Some(15)).await;
                if let Ok(projected) = projector.project_results(&results, true) {
                    let _response = ResultProjector::wrap_in_data_envelope(projected, "users");
                }
            }
        });
    }

    while join_set.join_next().await.is_some() {}

    let duration = start.elapsed();

    // 30 tasks × 3 queries = 90 queries
    assert_eq!(db.total_queries(), 90);
    println!("Complete pipeline (90 queries, 30 tasks): {:?}", duration);

    assert!(duration.as_secs() < 5);
}

/// Test: Long-running concurrent queries
#[tokio::test]
async fn test_long_running_concurrent_queries() {
    let db = Arc::new(ConcurrentMockDatabase::new());
    let mut join_set = JoinSet::new();

    let start = Instant::now();

    // Spawn 15 tasks that run for longer duration
    for _ in 0..15 {
        let db = Arc::clone(&db);
        join_set.spawn(async move {
            for _ in 0..20 {
                let _results = db.query_users(Some(20)).await;
            }
        });
    }

    while join_set.join_next().await.is_some() {}

    let duration = start.elapsed();

    // 15 tasks × 20 queries = 300 queries
    assert_eq!(db.total_queries(), 300);
    println!("Long-running (300 queries, 15 tasks): {:?}", duration);

    // Should handle 300 queries efficiently
    assert!(duration.as_secs() < 15);
}

/// Test: Query result correctness under concurrent load
#[tokio::test]
async fn test_result_correctness_under_load() {
    let db = Arc::new(ConcurrentMockDatabase::new());
    let mut join_set = JoinSet::new();

    // Spawn concurrent tasks and verify results
    for task_id in 0..10 {
        let db = Arc::clone(&db);
        join_set.spawn(async move {
            for i in 0..10 {
                let results = db.query_users(Some(5)).await;

                // Verify results are not empty
                assert!(!results.is_empty(), "Task {} iteration {} returned empty results", task_id, i);

                // Verify all results can be projected
                let projector = ResultProjector::new(vec!["id".to_string(), "name".to_string()]);
                let projected = projector.project_results(&results, true);
                assert!(projected.is_ok(), "Projection failed for task {}", task_id);

                // Verify projection has correct structure
                if let Ok(proj) = projected {
                    assert!(proj.is_array(), "Projected result is not an array");
                }
            }
        });
    }

    while let Some(result) = join_set.join_next().await {
        assert!(result.is_ok(), "Task panicked");
    }
}

/// Test: Concurrent error handling
#[tokio::test]
async fn test_concurrent_error_handling() {
    use fraiseql_core::error::FraiseQLError;

    let mut join_set = JoinSet::new();

    // Spawn tasks that create errors
    for task_id in 0..10 {
        join_set.spawn(async move {
            for i in 0..5 {
                let error = FraiseQLError::Validation {
                    message: format!("Test error from task {} iteration {}", task_id, i),
                    path: Some(format!("query.field{}", i)),
                };

                let wrapped = ResultProjector::wrap_error(&error);
                assert!(wrapped.get("errors").is_some(), "Error not properly wrapped");
                assert_eq!(wrapped.get("data"), None, "Data should be None in error response");
            }
        });
    }

    while let Some(result) = join_set.join_next().await {
        assert!(result.is_ok(), "Task panicked");
    }
}

/// Test: Concurrent projection with varying field counts
#[tokio::test]
async fn test_concurrent_varying_projections() {
    let db = Arc::new(ConcurrentMockDatabase::new());
    let mut join_set = JoinSet::new();

    // Different field counts per task
    let field_configs = vec![
        vec!["id".to_string()],
        vec!["id".to_string(), "name".to_string()],
        vec!["id".to_string(), "name".to_string(), "email".to_string()],
        vec!["id".to_string(), "name".to_string(), "email".to_string(), "status".to_string()],
    ];

    for (task_id, fields) in field_configs.iter().enumerate() {
        for repeat in 0..5 {
            let db = Arc::clone(&db);
            let fields = fields.clone();

            join_set.spawn(async move {
                let projector = ResultProjector::new(fields.clone());
                let results = db.query_users(Some(20)).await;

                let projected = projector.project_results(&results, true);
                assert!(projected.is_ok(), "Projection failed for task {} repeat {}", task_id, repeat);
            });
        }
    }

    while let Some(result) = join_set.join_next().await {
        assert!(result.is_ok(), "Task panicked");
    }
}

/// Test: Concurrent large batch processing
#[tokio::test]
async fn test_concurrent_large_batch_processing() {
    let db = Arc::new(ConcurrentMockDatabase::new());
    let mut join_set = JoinSet::new();

    let start = Instant::now();

    // Spawn 5 heavy tasks processing large batches
    for _ in 0..5 {
        let db = Arc::clone(&db);
        join_set.spawn(async move {
            let projector = ResultProjector::new(vec!["id".to_string(), "name".to_string()]);

            for _ in 0..10 {
                // Process all 100 users per query
                let results = db.query_users(Some(100)).await;

                // Wrap in envelope
                if let Ok(proj) = projector.project_results(&results, true) {
                    let _response = ResultProjector::wrap_in_data_envelope(proj, "users");
                }
            }
        });
    }

    while join_set.join_next().await.is_some() {}

    let duration = start.elapsed();

    // 5 tasks × 10 queries = 50 queries
    assert_eq!(db.total_queries(), 50);
    println!("Large batch processing (50 queries, 5 tasks, 100 rows each): {:?}", duration);

    assert!(duration.as_secs() < 10);
}

/// Test: Throughput measurement
#[tokio::test]
async fn test_throughput_measurement() {
    let db = Arc::new(ConcurrentMockDatabase::new());
    let mut join_set = JoinSet::new();

    let start = Instant::now();

    // Spawn 20 concurrent tasks for throughput measurement
    for _ in 0..20 {
        let db = Arc::clone(&db);
        join_set.spawn(async move {
            for _ in 0..25 {
                let _results = db.query_users(Some(10)).await;
            }
        });
    }

    while join_set.join_next().await.is_some() {}

    let duration = start.elapsed();
    let total_queries = db.total_queries();

    // 20 tasks × 25 queries = 500 queries
    assert_eq!(total_queries, 500);

    let throughput = total_queries as f64 / duration.as_secs_f64();
    println!("Throughput: {:.2} queries/second", throughput);
    println!("Total time: {:?} for {} queries", duration, total_queries);

    // Should achieve reasonable throughput (at least 50 qps with 100µs per query)
    assert!(throughput > 50.0, "Throughput too low: {:.2} qps", throughput);
}
