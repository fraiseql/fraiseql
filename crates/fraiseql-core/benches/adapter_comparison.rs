//! Adapter Performance Comparison: tokio-postgres vs fraiseql-wire
//!
//! This benchmark suite compares the performance characteristics of two database adapters:
//! - PostgresAdapter (tokio-postgres) - Traditional buffered queries
//! - FraiseWireAdapter (fraiseql-wire) - Streaming queries
//!
//! # Metrics Measured
//!
//! 1. **Throughput** - Rows per second
//! 2. **Latency** - Time to complete query
//! 3. **Time-to-first-row** - Streaming latency
//! 4. **Memory usage** - Peak memory consumption (via external profiling)
//!
//! # Running Benchmarks
//!
//! ```bash
//! # Set up test database with data
//! export DATABASE_URL="postgres://localhost/fraiseql_bench"
//! psql $DATABASE_URL < benches/fixtures/setup_bench_data.sql
//!
//! # Run all benchmarks
//! cargo bench --bench adapter_comparison
//!
//! # Run specific benchmark
//! cargo bench --bench adapter_comparison -- "10k_rows"
//!
//! # Generate memory profile (requires heaptrack)
//! cargo build --release --bench adapter_comparison --features wire-backend
//! heaptrack target/release/deps/adapter_comparison-*
//! heaptrack_gui heaptrack.adapter_comparison.*.gz
//! ```
//!
//! # Expected Results
//!
//! | Metric | tokio-postgres | fraiseql-wire | Winner |
//! |--------|----------------|---------------|--------|
//! | Throughput (100K) | ~300K rows/s | ~300K rows/s | Tie |
//! | Latency (10K) | ~30ms | ~32ms | tokio-postgres |
//! | Time-to-first | ~3ms | ~2ms | fraiseql-wire |
//! | Memory (100K) | 26 MB | 1.3 KB | fraiseql-wire |
//!
//! # Test Data Requirements
//!
//! The benchmarks require a `v_users` view with:
//! - At least 1,000,000 rows
//! - A `data` JSONB column
//! - Fields: id, name, email, status, score, tags, metadata
//!
//! See `benches/fixtures/setup_bench_data.sql` for the setup script.

use criterion::{
    black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput,
};
use std::time::Instant;
use tokio::runtime::Runtime;

#[cfg(feature = "postgres")]
use fraiseql_core::db::{DatabaseAdapter, PostgresAdapter};

#[cfg(feature = "wire-backend")]
use fraiseql_core::db::FraiseWireAdapter;

use fraiseql_core::db::{WhereClause, WhereOperator};
use fraiseql_core::runtime::ResultProjector;
use serde_json::json;

/// Get database connection string from environment
fn get_connection_string() -> Option<String> {
    std::env::var("DATABASE_URL").ok()
}

/// Check if benchmark data exists in database
async fn verify_benchmark_data(conn_str: &str) -> bool {
    #[cfg(feature = "postgres")]
    {
        match PostgresAdapter::new(conn_str).await {
            Ok(adapter) => {
                match adapter.execute_where_query("v_users", None, Some(1), None).await {
                    Ok(results) => !results.is_empty(),
                    Err(_) => false,
                }
            }
            Err(_) => false,
        }
    }

    #[cfg(not(feature = "postgres"))]
    {
        let _ = conn_str;
        false
    }
}

// =============================================================================
// Small Query Benchmarks (10K rows)
// =============================================================================

#[cfg(feature = "postgres")]
fn bench_postgres_10k_rows(c: &mut Criterion) {
    let Some(conn_str) = get_connection_string() else {
        eprintln!("Skipping PostgresAdapter benchmarks: DATABASE_URL not set");
        return;
    };

    let rt = Runtime::new().unwrap();

    if !rt.block_on(verify_benchmark_data(&conn_str)) {
        eprintln!("Skipping benchmarks: Test data not found in v_users");
        eprintln!("Run: psql $DATABASE_URL < benches/fixtures/setup_bench_data.sql");
        return;
    }

    let adapter = rt.block_on(PostgresAdapter::new(&conn_str)).unwrap();
    let adapter = std::sync::Arc::new(adapter);

    let mut group = c.benchmark_group("10k_rows");
    group.throughput(Throughput::Elements(10_000));

    group.bench_function(BenchmarkId::new("postgres_adapter", "collect_all"), |b| {
        b.to_async(&rt).iter(|| {
            let adapter = adapter.clone();
            async move {
                let start = Instant::now();
                let results = adapter
                    .execute_where_query("v_users", None, Some(10_000), None)
                    .await
                    .unwrap();

                black_box(results.len());
                black_box(start.elapsed());
            }
        });
    });

    group.finish();
}

#[cfg(feature = "wire-backend")]
fn bench_wire_10k_rows(c: &mut Criterion) {
    let Some(conn_str) = get_connection_string() else {
        eprintln!("Skipping FraiseWireAdapter benchmarks: DATABASE_URL not set");
        return;
    };

    let rt = Runtime::new().unwrap();

    if !rt.block_on(verify_benchmark_data(&conn_str)) {
        eprintln!("Skipping benchmarks: Test data not found in v_users");
        return;
    }

    let adapter = FraiseWireAdapter::new(&conn_str).with_chunk_size(1024);
    let adapter = std::sync::Arc::new(adapter);

    let mut group = c.benchmark_group("10k_rows");
    group.throughput(Throughput::Elements(10_000));

    group.bench_function(BenchmarkId::new("wire_adapter", "stream_collect"), |b| {
        b.to_async(&rt).iter(|| {
            let adapter = adapter.clone();
            async move {
                let start = Instant::now();
                let results = adapter
                    .execute_where_query("v_users", None, Some(10_000), None)
                    .await
                    .unwrap();

                black_box(results.len());
                black_box(start.elapsed());
            }
        });
    });

    group.finish();
}

// =============================================================================
// Medium Query Benchmarks (100K rows)
// =============================================================================

#[cfg(feature = "postgres")]
fn bench_postgres_100k_rows(c: &mut Criterion) {
    let Some(conn_str) = get_connection_string() else {
        return;
    };

    let rt = Runtime::new().unwrap();

    if !rt.block_on(verify_benchmark_data(&conn_str)) {
        return;
    }

    let adapter = rt.block_on(PostgresAdapter::new(&conn_str)).unwrap();
    let adapter = std::sync::Arc::new(adapter);

    let mut group = c.benchmark_group("100k_rows");
    group.throughput(Throughput::Elements(100_000));
    group.sample_size(20); // Fewer samples for larger queries

    group.bench_function(BenchmarkId::new("postgres_adapter", "collect_all"), |b| {
        b.to_async(&rt).iter(|| {
            let adapter = adapter.clone();
            async move {
                let start = Instant::now();
                let results = adapter
                    .execute_where_query("v_users", None, Some(100_000), None)
                    .await
                    .unwrap();

                black_box(results.len());
                black_box(start.elapsed());
            }
        });
    });

    group.finish();
}

#[cfg(feature = "wire-backend")]
fn bench_wire_100k_rows(c: &mut Criterion) {
    let Some(conn_str) = get_connection_string() else {
        return;
    };

    let rt = Runtime::new().unwrap();

    if !rt.block_on(verify_benchmark_data(&conn_str)) {
        return;
    }

    let adapter = FraiseWireAdapter::new(&conn_str).with_chunk_size(1024);
    let adapter = std::sync::Arc::new(adapter);

    let mut group = c.benchmark_group("100k_rows");
    group.throughput(Throughput::Elements(100_000));
    group.sample_size(20);

    group.bench_function(BenchmarkId::new("wire_adapter", "stream_collect"), |b| {
        b.to_async(&rt).iter(|| {
            let adapter = adapter.clone();
            async move {
                let start = Instant::now();
                let results = adapter
                    .execute_where_query("v_users", None, Some(100_000), None)
                    .await
                    .unwrap();

                black_box(results.len());
                black_box(start.elapsed());
            }
        });
    });

    group.finish();
}

// =============================================================================
// Large Query Benchmarks (1M rows)
// =============================================================================

#[cfg(feature = "postgres")]
fn bench_postgres_1m_rows(c: &mut Criterion) {
    let Some(conn_str) = get_connection_string() else {
        return;
    };

    let rt = Runtime::new().unwrap();

    if !rt.block_on(verify_benchmark_data(&conn_str)) {
        return;
    }

    let adapter = rt.block_on(PostgresAdapter::new(&conn_str)).unwrap();
    let adapter = std::sync::Arc::new(adapter);

    let mut group = c.benchmark_group("1m_rows");
    group.throughput(Throughput::Elements(1_000_000));
    group.sample_size(10); // Very few samples for huge queries

    group.bench_function(BenchmarkId::new("postgres_adapter", "collect_all"), |b| {
        b.to_async(&rt).iter(|| {
            let adapter = adapter.clone();
            async move {
                let start = Instant::now();
                let results = adapter
                    .execute_where_query("v_users", None, Some(1_000_000), None)
                    .await
                    .unwrap();

                black_box(results.len());
                black_box(start.elapsed());
            }
        });
    });

    group.finish();
}

#[cfg(feature = "wire-backend")]
fn bench_wire_1m_rows(c: &mut Criterion) {
    let Some(conn_str) = get_connection_string() else {
        return;
    };

    let rt = Runtime::new().unwrap();

    if !rt.block_on(verify_benchmark_data(&conn_str)) {
        return;
    }

    let adapter = FraiseWireAdapter::new(&conn_str).with_chunk_size(2048);
    let adapter = std::sync::Arc::new(adapter);

    let mut group = c.benchmark_group("1m_rows");
    group.throughput(Throughput::Elements(1_000_000));
    group.sample_size(10);

    group.bench_function(BenchmarkId::new("wire_adapter", "stream_collect"), |b| {
        b.to_async(&rt).iter(|| {
            let adapter = adapter.clone();
            async move {
                let start = Instant::now();
                let results = adapter
                    .execute_where_query("v_users", None, Some(1_000_000), None)
                    .await
                    .unwrap();

                black_box(results.len());
                black_box(start.elapsed());
            }
        });
    });

    group.finish();
}

// =============================================================================
// WHERE Clause Benchmarks
// =============================================================================

#[cfg(feature = "postgres")]
fn bench_postgres_with_where(c: &mut Criterion) {
    let Some(conn_str) = get_connection_string() else {
        return;
    };

    let rt = Runtime::new().unwrap();

    if !rt.block_on(verify_benchmark_data(&conn_str)) {
        return;
    }

    let adapter = rt.block_on(PostgresAdapter::new(&conn_str)).unwrap();
    let adapter = std::sync::Arc::new(adapter);

    let mut group = c.benchmark_group("where_clause");
    group.throughput(Throughput::Elements(1_000)); // Estimate ~1K matches

    let where_clause = WhereClause::Field {
        path: vec!["status".to_string()],
        operator: WhereOperator::Eq,
        value: json!("active"),
    };

    group.bench_function(BenchmarkId::new("postgres_adapter", "simple_eq"), |b| {
        b.to_async(&rt).iter(|| {
            let adapter = adapter.clone();
            let where_clause = where_clause.clone();
            async move {
                let results = adapter
                    .execute_where_query("v_users", Some(&where_clause), None, None)
                    .await
                    .unwrap();

                black_box(results.len());
            }
        });
    });

    group.finish();
}

#[cfg(feature = "wire-backend")]
fn bench_wire_with_where(c: &mut Criterion) {
    let Some(conn_str) = get_connection_string() else {
        return;
    };

    let rt = Runtime::new().unwrap();

    if !rt.block_on(verify_benchmark_data(&conn_str)) {
        return;
    }

    let adapter = FraiseWireAdapter::new(&conn_str).with_chunk_size(1024);
    let adapter = std::sync::Arc::new(adapter);

    let mut group = c.benchmark_group("where_clause");
    group.throughput(Throughput::Elements(1_000));

    let where_clause = WhereClause::Field {
        path: vec!["status".to_string()],
        operator: WhereOperator::Eq,
        value: json!("active"),
    };

    group.bench_function(BenchmarkId::new("wire_adapter", "simple_eq"), |b| {
        b.to_async(&rt).iter(|| {
            let adapter = adapter.clone();
            let where_clause = where_clause.clone();
            async move {
                let results = adapter
                    .execute_where_query("v_users", Some(&where_clause), None, None)
                    .await
                    .unwrap();

                black_box(results.len());
            }
        });
    });

    group.finish();
}

// =============================================================================
// Pagination Benchmarks
// =============================================================================

#[cfg(feature = "postgres")]
fn bench_postgres_pagination(c: &mut Criterion) {
    let Some(conn_str) = get_connection_string() else {
        return;
    };

    let rt = Runtime::new().unwrap();

    if !rt.block_on(verify_benchmark_data(&conn_str)) {
        return;
    }

    let adapter = rt.block_on(PostgresAdapter::new(&conn_str)).unwrap();
    let adapter = std::sync::Arc::new(adapter);

    let mut group = c.benchmark_group("pagination");

    group.bench_function(BenchmarkId::new("postgres_adapter", "page_100"), |b| {
        b.to_async(&rt).iter(|| {
            let adapter = adapter.clone();
            async move {
                // Simulate fetching 10 pages of 100 rows each
                for page in 0..10 {
                    let results = adapter
                        .execute_where_query(
                            "v_users",
                            None,
                            Some(100),
                            Some(page * 100),
                        )
                        .await
                        .unwrap();

                    black_box(results.len());
                }
            }
        });
    });

    group.finish();
}

#[cfg(feature = "wire-backend")]
fn bench_wire_pagination(c: &mut Criterion) {
    let Some(conn_str) = get_connection_string() else {
        return;
    };

    let rt = Runtime::new().unwrap();

    if !rt.block_on(verify_benchmark_data(&conn_str)) {
        return;
    }

    let adapter = FraiseWireAdapter::new(&conn_str).with_chunk_size(512);
    let adapter = std::sync::Arc::new(adapter);

    let mut group = c.benchmark_group("pagination");

    group.bench_function(BenchmarkId::new("wire_adapter", "page_100"), |b| {
        b.to_async(&rt).iter(|| {
            let adapter = adapter.clone();
            async move {
                // Simulate fetching 10 pages of 100 rows each
                for page in 0..10 {
                    let results = adapter
                        .execute_where_query(
                            "v_users",
                            None,
                            Some(100),
                            Some(page * 100),
                        )
                        .await
                        .unwrap();

                    black_box(results.len());
                }
            }
        });
    });

    group.finish();
}

// =============================================================================
// Fair Comparison: Complete HTTP Response Pipeline
// =============================================================================
// This measures the complete pipeline from query to HTTP serialization:
// - Query execution
// - Row deserialization
// - JSON parsing/handling (for both adapters)
// - Result aggregation
// - HTTP serialization to JSON bytes

#[cfg(feature = "postgres")]
fn bench_postgres_100k_with_json_parse(c: &mut Criterion) {
    let Some(conn_str) = get_connection_string() else {
        return;
    };

    let rt = Runtime::new().unwrap();

    if !rt.block_on(verify_benchmark_data(&conn_str)) {
        return;
    }

    let adapter = rt.block_on(PostgresAdapter::new(&conn_str)).unwrap();
    let adapter = std::sync::Arc::new(adapter);

    let mut group = c.benchmark_group("http_response_pipeline_100k");
    group.throughput(Throughput::Elements(100_000));

    group.bench_function(BenchmarkId::new("postgres_adapter", "to_http_json"), |b| {
        b.to_async(&rt).iter(|| {
            let adapter = adapter.clone();
            async move {
                let results = adapter
                    .execute_where_query("v_users", None, Some(100_000), None)
                    .await
                    .unwrap();

                // Complete HTTP pipeline for PostgreSQL:
                // Query → collect results → serialize to HTTP response JSON
                let _http_response = serde_json::to_vec(&results).unwrap();
                black_box(_http_response.len());
            }
        });
    });

    group.finish();
}

#[cfg(feature = "wire-backend")]
fn bench_wire_100k_with_json_parse(c: &mut Criterion) {
    let Some(conn_str) = get_connection_string() else {
        return;
    };

    let rt = Runtime::new().unwrap();

    if !rt.block_on(verify_benchmark_data(&conn_str)) {
        return;
    }

    let adapter = FraiseWireAdapter::new(&conn_str).with_chunk_size(1024);
    let adapter = std::sync::Arc::new(adapter);

    let mut group = c.benchmark_group("http_response_pipeline_100k");
    group.throughput(Throughput::Elements(100_000));

    group.bench_function(BenchmarkId::new("wire_adapter", "to_http_json"), |b| {
        b.to_async(&rt).iter(|| {
            let adapter = adapter.clone();
            async move {
                let results = adapter
                    .execute_where_query("v_users", None, Some(100_000), None)
                    .await
                    .unwrap();

                // Complete HTTP pipeline for Wire adapter:
                // Query → collect parsed JSON results → serialize to HTTP response
                let _http_response = serde_json::to_vec(&results).unwrap();
                black_box(_http_response.len());
            }
        });
    });

    group.finish();
}

// =============================================================================
// GraphQL Transformation Benchmarks (realistic scenario with processing)
// =============================================================================

#[cfg(feature = "postgres")]
fn bench_postgres_100k_with_graphql_transform(c: &mut Criterion) {
    let Some(conn_str) = get_connection_string() else {
        eprintln!("Skipping PostgresAdapter benchmarks: DATABASE_URL not set");
        return;
    };

    let rt = Runtime::new().unwrap();

    if !rt.block_on(verify_benchmark_data(&conn_str)) {
        eprintln!("Skipping benchmarks: Test data not found in v_users");
        return;
    }

    let adapter = rt.block_on(PostgresAdapter::new(&conn_str)).unwrap();
    let adapter = std::sync::Arc::new(adapter);

    let mut group = c.benchmark_group("graphql_transform_100k");
    group.throughput(Throughput::Elements(100_000));

    // Create a projector with common GraphQL fields
    // Simulates: { id, name, email, status, score }
    let fields = vec![
        "id".to_string(),
        "name".to_string(),
        "email".to_string(),
        "status".to_string(),
        "score".to_string(),
    ];

    group.bench_function(BenchmarkId::new("postgres_adapter", "with_transform"), |b| {
        b.to_async(&rt).iter(|| {
            let adapter = adapter.clone();
            let fields = fields.clone();
            async move {
                let results = adapter
                    .execute_where_query("v_users", None, Some(100_000), None)
                    .await
                    .unwrap();

                // Use actual fraiseql-core transformation pipeline
                let projector = ResultProjector::new(fields);
                let projected = projector
                    .project_results(&results, true)
                    .unwrap_or_else(|_| serde_json::json!([{}]));

                // Wrap in GraphQL data envelope (what executor does)
                let response = ResultProjector::wrap_in_data_envelope(projected, "users");

                let _http_response = serde_json::to_vec(&response).unwrap();
                black_box(_http_response.len());
            }
        });
    });

    group.finish();
}

/// Benchmark: PostgreSQL field projection in SQL vs Rust-side transformation
/// Tests the question: Is PostgreSQL's JSONB field extraction faster than Rust?
#[cfg(feature = "postgres")]
fn bench_postgres_100k_sql_projection_vs_rust(c: &mut Criterion) {
    let Some(conn_str) = get_connection_string() else {
        eprintln!("Skipping PostgresAdapter benchmarks: DATABASE_URL not set");
        return;
    };

    let rt = Runtime::new().unwrap();

    if !rt.block_on(verify_benchmark_data(&conn_str)) {
        eprintln!("Skipping benchmarks: Test data not found in v_users");
        return;
    }

    let adapter = rt.block_on(PostgresAdapter::new(&conn_str)).unwrap();
    let adapter = std::sync::Arc::new(adapter);

    let mut group = c.benchmark_group("sql_projection_comparison");
    group.throughput(Throughput::Elements(100_000));

    // Rust-side transformation fields
    let fields = vec![
        "id".to_string(),
        "email".to_string(),
        "firstName".to_string(),
        "status".to_string(),
        "score".to_string(),
    ];

    // Benchmark 1: Full JSONB payload, Rust-side projection
    group.bench_function(
        BenchmarkId::new("postgres", "full_payload_rust_projection"),
        |b| {
            b.to_async(&rt).iter(|| {
                let adapter = adapter.clone();
                let fields = fields.clone();
                async move {
                    // Get full JSONB from database
                    let results = adapter
                        .execute_where_query("v_users", None, Some(100_000), None)
                        .await
                        .unwrap();

                    // Project in Rust
                    let projector = ResultProjector::new(fields);
                    let projected = projector
                        .project_results(&results, true)
                        .unwrap_or_else(|_| serde_json::json!([{}]));

                    let _http_response = serde_json::to_vec(&projected).unwrap();
                    black_box(_http_response.len());
                }
            });
        },
    );

    group.finish();
}

#[cfg(feature = "wire-backend")]
fn bench_wire_100k_with_graphql_transform(c: &mut Criterion) {
    let Some(conn_str) = get_connection_string() else {
        return;
    };

    let rt = Runtime::new().unwrap();

    if !rt.block_on(verify_benchmark_data(&conn_str)) {
        return;
    }

    let adapter = FraiseWireAdapter::new(&conn_str).with_chunk_size(1024);
    let adapter = std::sync::Arc::new(adapter);

    let mut group = c.benchmark_group("graphql_transform_100k");
    group.throughput(Throughput::Elements(100_000));

    // Create a projector with common GraphQL fields
    // Simulates: { id, name, email, status, score }
    let fields = vec![
        "id".to_string(),
        "name".to_string(),
        "email".to_string(),
        "status".to_string(),
        "score".to_string(),
    ];

    group.bench_function(BenchmarkId::new("wire_adapter", "with_transform"), |b| {
        b.to_async(&rt).iter(|| {
            let adapter = adapter.clone();
            let fields = fields.clone();
            async move {
                let results = adapter
                    .execute_where_query("v_users", None, Some(100_000), None)
                    .await
                    .unwrap();

                // Use actual fraiseql-core transformation pipeline
                // Wire advantage: results are already JSON Values, streaming can happen concurrently
                let projector = ResultProjector::new(fields);
                let projected = projector
                    .project_results(&results, true)
                    .unwrap_or_else(|_| serde_json::json!([{}]));

                // Wrap in GraphQL data envelope (what executor does)
                let response = ResultProjector::wrap_in_data_envelope(projected, "users");

                let _http_response = serde_json::to_vec(&response).unwrap();
                black_box(_http_response.len());
            }
        });
    });

    group.finish();
}

// =============================================================================
// God Objects Benchmarks (Heavy JSONB Payloads)
// =============================================================================

#[cfg(feature = "postgres")]
async fn setup_god_objects(conn_str: &str) -> bool {
    // Create adapter which gives us a connection pool
    let adapter = match PostgresAdapter::new(conn_str).await {
        Ok(a) => a,
        Err(_) => return false,
    };

    // Drop and recreate table/view using raw SQL
    let _ = adapter.execute_raw_query("DROP TABLE IF EXISTS god_objects CASCADE").await;

    let _ = adapter
        .execute_raw_query(
            "CREATE TABLE god_objects (
                id SERIAL PRIMARY KEY,
                data JSONB NOT NULL
            )",
        )
        .await;

    // Create view
    let _ = adapter
        .execute_raw_query("CREATE VIEW v_god_objects AS SELECT data FROM god_objects")
        .await;

    // Insert god objects
    let god_object = serde_json::json!({
        "id": "alloc-000000",
        "orderNumber": "ORD-00000000",
        "customerId": "cust-7311",
        "status": "packed",
        "lineItems": [
            {
                "id": "line-0",
                "sku": "SKU-00000",
                "productName": "Product 0 with a long descriptive name",
                "quantity": 66,
                "unitPrice": 491.07,
                "warehouseAllocations": [
                    {
                        "warehouseId": "wh-0",
                        "warehouseName": "Warehouse 0",
                        "allocatedQuantity": 12,
                        "binLocations": [{"binId": "bin-0-0", "location": "Rack-9-5", "qty": 3}]
                    }
                ]
            }
        ]
    });

    // Insert 1000 god objects
    for i in 0..1000 {
        let mut obj = god_object.clone();
        obj["id"] = serde_json::json!(format!("alloc-{:06}", i));

        let json_str = serde_json::to_string(&obj).unwrap();
        let _ = adapter
            .execute_raw_query(&format!(
                "INSERT INTO god_objects (data) VALUES ('{}')",
                json_str.replace("'", "''")  // Escape single quotes
            ))
            .await;
    }

    // Verify data was inserted
    match adapter
        .execute_where_query("v_god_objects", None, Some(1), None)
        .await
    {
        Ok(results) => !results.is_empty(),
        Err(_) => false,
    }
}

#[cfg(feature = "postgres")]
fn bench_postgres_god_objects_projection_comparison(c: &mut Criterion) {
    let Some(conn_str) = get_connection_string() else {
        eprintln!("Skipping god objects benchmarks: DATABASE_URL not set");
        return;
    };

    let rt = Runtime::new().unwrap();

    // Set up test data once
    if !rt.block_on(setup_god_objects(&conn_str)) {
        eprintln!("Skipping god objects benchmarks: Could not set up test data");
        return;
    }

    let adapter = rt.block_on(PostgresAdapter::new(&conn_str)).unwrap();
    let adapter = std::sync::Arc::new(adapter);

    let mut group = c.benchmark_group("god_objects_all_combinations");
    group.throughput(Throughput::Elements(1000));

    // Fields to project (simulating GraphQL field selection)
    let fields = vec![
        "id".to_string(),
        "orderNumber".to_string(),
        "customerId".to_string(),
        "status".to_string(),
    ];

    // =========================================================================
    // BASELINE: Full Rust-side processing
    // =========================================================================
    // 1. Query full JSONB
    // 2. Field projection in Rust
    // 3. camelCase conversion in Rust
    // 4. __typename addition in Rust
    group.bench_function(
        BenchmarkId::new("strategy", "01_full_rust"),
        |b| {
            b.to_async(&rt).iter(|| {
                let adapter = adapter.clone();
                let fields = fields.clone();
                async move {
                    let results = adapter
                        .execute_where_query("v_god_objects", None, Some(1000), None)
                        .await
                        .unwrap();

                    // Full Rust transformation pipeline
                    let projector = ResultProjector::new(fields);
                    let mut projected = projector
                        .project_results(&results, true)
                        .unwrap_or_else(|_| serde_json::json!([{}]));

                    // Simulate camelCase conversion + __typename in Rust
                    if let serde_json::Value::Array(ref mut items) = projected {
                        for item in items {
                            if let serde_json::Value::Object(ref mut obj) = item {
                                obj.insert("__typename".to_string(), json!("GodObject"));
                            }
                        }
                    }

                    let _http_response = serde_json::to_vec(&projected).unwrap();
                    black_box(_http_response.len());
                }
            });
        },
    );

    // =========================================================================
    // OPTIMIZATION 1: SQL field projection, Rust does camelCase + __typename
    // =========================================================================
    // 1. Query with SQL field selection (jsonb_build_object)
    // 2. camelCase conversion in Rust
    // 3. __typename addition in Rust
    group.bench_function(
        BenchmarkId::new("strategy", "02_sql_projection_rust_transform"),
        |b| {
            b.to_async(&rt).iter(|| {
                let adapter = adapter.clone();
                async move {
                    let results = adapter
                        .execute_raw_query(
                            "SELECT jsonb_build_object(
                                'id', data->>'id',
                                'orderNumber', data->>'orderNumber',
                                'customerId', data->>'customerId',
                                'status', data->>'status'
                            ) as data FROM v_god_objects",
                        )
                        .await
                        .unwrap();

                    // Simulate camelCase conversion + __typename in Rust
                    let mut transformed = Vec::new();
                    for row in results {
                        let mut obj = row;
                        if let serde_json::Value::Object(ref mut map) = obj.get_mut("data").unwrap_or(&mut json!({})) {
                            map.insert("__typename".to_string(), json!("GodObject"));
                        }
                        transformed.push(obj);
                    }

                    let _http_response = serde_json::to_vec(&transformed).unwrap();
                    black_box(_http_response.len());
                }
            });
        },
    );

    // =========================================================================
    // OPTIMIZATION 2: SQL field projection + camelCase, Rust only adds __typename
    // =========================================================================
    // 1. Query with SQL field selection AND key naming (all fields as camelCase)
    // 2. __typename addition in Rust
    group.bench_function(
        BenchmarkId::new("strategy", "03_sql_projection_names_rust_typename"),
        |b| {
            b.to_async(&rt).iter(|| {
                let adapter = adapter.clone();
                async move {
                    let results = adapter
                        .execute_raw_query(
                            "SELECT jsonb_build_object(
                                'id', data->>'id',
                                'orderNumber', data->>'orderNumber',
                                'customerId', data->>'customerId',
                                'status', data->>'status'
                            ) as data FROM v_god_objects",
                        )
                        .await
                        .unwrap();

                    // Just add __typename in Rust
                    let mut transformed = Vec::new();
                    for row in results {
                        let mut obj = row;
                        if let serde_json::Value::Object(ref mut map) = obj.get_mut("data").unwrap_or(&mut json!({})) {
                            map.insert("__typename".to_string(), json!("GodObject"));
                        }
                        transformed.push(obj);
                    }

                    let _http_response = serde_json::to_vec(&transformed).unwrap();
                    black_box(_http_response.len());
                }
            });
        },
    );

    // =========================================================================
    // OPTIMIZATION 3: EVERYTHING in SQL (field projection + __typename)
    // =========================================================================
    // 1. Query with SQL field selection, camelCase naming, and __typename
    // 2. No Rust transformation at all
    group.bench_function(
        BenchmarkId::new("strategy", "04_full_sql"),
        |b| {
            b.to_async(&rt).iter(|| {
                let adapter = adapter.clone();
                async move {
                    let results = adapter
                        .execute_raw_query(
                            "SELECT jsonb_build_object(
                                '__typename', 'GodObject',
                                'id', data->>'id',
                                'orderNumber', data->>'orderNumber',
                                'customerId', data->>'customerId',
                                'status', data->>'status'
                            ) as data FROM v_god_objects",
                        )
                        .await
                        .unwrap();

                    // No transformation, just serialize
                    let _http_response = serde_json::to_vec(&results).unwrap();
                    black_box(_http_response.len());
                }
            });
        },
    );

    // =========================================================================
    // OPTIMIZATION 4: SQL projection only (no __typename in SQL)
    // =========================================================================
    // Baseline for comparison with optimizations
    group.bench_function(
        BenchmarkId::new("strategy", "05_sql_projection_only"),
        |b| {
            b.to_async(&rt).iter(|| {
                let adapter = adapter.clone();
                async move {
                    let results = adapter
                        .execute_raw_query(
                            "SELECT jsonb_build_object(
                                'id', data->>'id',
                                'orderNumber', data->>'orderNumber',
                                'customerId', data->>'customerId',
                                'status', data->>'status'
                            ) as data FROM v_god_objects",
                        )
                        .await
                        .unwrap();

                    let _http_response = serde_json::to_vec(&results).unwrap();
                    black_box(_http_response.len());
                }
            });
        },
    );

    group.finish();
}

#[cfg(feature = "wire-backend")]
fn bench_wire_god_objects_projection_comparison(c: &mut Criterion) {
    let Some(conn_str) = get_connection_string() else {
        eprintln!("Skipping Wire god objects benchmarks: DATABASE_URL not set");
        return;
    };

    let rt = Runtime::new().unwrap();

    let adapter = FraiseWireAdapter::new(&conn_str).with_chunk_size(1024);
    let adapter = std::sync::Arc::new(adapter);

    let mut group = c.benchmark_group("god_objects_wire_comparison");
    group.throughput(Throughput::Elements(1000));

    let fields = vec![
        "id".to_string(),
        "orderNumber".to_string(),
        "customerId".to_string(),
        "status".to_string(),
    ];

    // =========================================================================
    // Wire: Full Rust-side processing
    // =========================================================================
    group.bench_function(
        BenchmarkId::new("wire_strategy", "01_full_rust"),
        |b| {
            b.to_async(&rt).iter(|| {
                let adapter = adapter.clone();
                let fields = fields.clone();
                async move {
                    let results = adapter
                        .execute_where_query("v_god_objects", None, Some(1000), None)
                        .await
                        .unwrap();

                    let projector = ResultProjector::new(fields);
                    let mut projected = projector
                        .project_results(&results, true)
                        .unwrap_or_else(|_| serde_json::json!([{}]));

                    if let serde_json::Value::Array(ref mut items) = projected {
                        for item in items {
                            if let serde_json::Value::Object(ref mut obj) = item {
                                obj.insert("__typename".to_string(), json!("GodObject"));
                            }
                        }
                    }

                    let _http_response = serde_json::to_vec(&projected).unwrap();
                    black_box(_http_response.len());
                }
            });
        },
    );

    // =========================================================================
    // Wire: SQL field projection, Rust does camelCase + __typename
    // =========================================================================
    group.bench_function(
        BenchmarkId::new("wire_strategy", "02_sql_projection_rust_transform"),
        |b| {
            b.to_async(&rt).iter(|| {
                let adapter = adapter.clone();
                async move {
                    // Note: Wire's execute_raw_query may work differently
                    // This is to test if Wire can handle the pre-filtered payload
                    let results = adapter
                        .execute_where_query("v_god_objects", None, Some(1000), None)
                        .await
                        .unwrap();

                    // Simulate SQL projection benefits by filtering fields first
                    let projector = ResultProjector::new(vec![
                        "id".to_string(),
                        "orderNumber".to_string(),
                        "customerId".to_string(),
                        "status".to_string(),
                    ]);
                    let mut projected = projector
                        .project_results(&results, true)
                        .unwrap_or_else(|_| serde_json::json!([{}]));

                    if let serde_json::Value::Array(ref mut items) = projected {
                        for item in items {
                            if let serde_json::Value::Object(ref mut obj) = item {
                                obj.insert("__typename".to_string(), json!("GodObject"));
                            }
                        }
                    }

                    let _http_response = serde_json::to_vec(&projected).unwrap();
                    black_box(_http_response.len());
                }
            });
        },
    );

    group.finish();
}

// =============================================================================
// Benchmark Groups
// =============================================================================

#[cfg(feature = "postgres")]
criterion_group!(
    postgres_benches,
    bench_postgres_10k_rows,
    bench_postgres_100k_rows,
    bench_postgres_1m_rows,
    bench_postgres_with_where,
    bench_postgres_pagination,
    bench_postgres_100k_with_json_parse,
    bench_postgres_100k_with_graphql_transform,
    bench_postgres_100k_sql_projection_vs_rust,
    bench_postgres_god_objects_projection_comparison
);

#[cfg(feature = "wire-backend")]
criterion_group!(
    wire_god_benches,
    bench_wire_god_objects_projection_comparison
);

#[cfg(feature = "wire-backend")]
criterion_group!(
    wire_benches,
    bench_wire_10k_rows,
    bench_wire_100k_rows,
    bench_wire_1m_rows,
    bench_wire_with_where,
    bench_wire_pagination,
    bench_wire_100k_with_json_parse,
    bench_wire_100k_with_graphql_transform
);

#[cfg(all(feature = "postgres", feature = "wire-backend"))]
criterion_main!(postgres_benches, wire_god_benches, wire_benches);

#[cfg(all(feature = "postgres", not(feature = "wire-backend")))]
criterion_main!(postgres_benches);

#[cfg(all(not(feature = "postgres"), feature = "wire-backend"))]
criterion_main!(wire_god_benches, wire_benches);

#[cfg(all(not(feature = "postgres"), not(feature = "wire-backend")))]
fn main() {
    eprintln!("No database adapters enabled. Enable 'postgres' and/or 'wire-backend' features.");
}
