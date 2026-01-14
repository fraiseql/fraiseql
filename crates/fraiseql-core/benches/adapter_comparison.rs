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
    bench_postgres_100k_with_json_parse
);

#[cfg(feature = "wire-backend")]
criterion_group!(
    wire_benches,
    bench_wire_10k_rows,
    bench_wire_100k_rows,
    bench_wire_1m_rows,
    bench_wire_with_where,
    bench_wire_pagination,
    bench_wire_100k_with_json_parse
);

#[cfg(all(feature = "postgres", feature = "wire-backend"))]
criterion_main!(postgres_benches, wire_benches);

#[cfg(all(feature = "postgres", not(feature = "wire-backend")))]
criterion_main!(postgres_benches);

#[cfg(all(not(feature = "postgres"), feature = "wire-backend"))]
criterion_main!(wire_benches);

#[cfg(all(not(feature = "postgres"), not(feature = "wire-backend")))]
fn main() {
    eprintln!("No database adapters enabled. Enable 'postgres' and/or 'wire-backend' features.");
}
