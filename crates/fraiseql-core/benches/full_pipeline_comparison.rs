//! Full Pipeline Performance Comparison: tokio-postgres vs fraiseql-wire
//!
//! This benchmark suite compares the **complete FraiseQL execution pipeline**:
//! 1. Database query execution
//! 2. Field projection (selecting only requested fields)
//! 3. Field name transformation (snake_case → camelCase)
//! 4. __typename addition
//! 5. GraphQL data envelope wrapping
//!
//! This simulates real-world GraphQL query execution to measure the actual
//! performance difference between PostgresAdapter (tokio-postgres) and
//! FraiseWireAdapter (fraiseql-wire) in production scenarios.

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
use fraiseql_core::runtime::projection::ResultProjector;
use fraiseql_core::utils::casing::to_camel_case;
use serde_json::{json, Map, Value};

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
                match adapter.execute_where_query("v_benchmark_data", None, Some(1), None).await {
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

/// Transform a single database row through FraiseQL pipeline
///
/// This simulates real GraphQL execution per-row:
/// 1. Project fields (select only requested fields)
/// 2. Transform snake_case keys to camelCase
/// 3. Add __typename field
fn transform_single_row(
    result_value: Value,
    requested_fields: &[&str],
    type_name: &str,
) -> Value {
    let mut projected = Map::new();

    if let Value::Object(obj) = result_value {
        for field in requested_fields {
            if let Some(value) = obj.get(*field) {
                // Transform snake_case to camelCase
                let camel_field = to_camel_case(field);
                projected.insert(camel_field, value.clone());
            }
        }
    }

    // Add __typename
    projected.insert("__typename".to_string(), Value::String(type_name.to_string()));

    Value::Object(projected)
}

/// Transform database results through full FraiseQL pipeline
///
/// This processes results in streaming fashion (as they arrive)
fn transform_results(
    results: Vec<Value>,
    requested_fields: &[&str],
    type_name: &str,
) -> Value {
    // Process each row as it arrives (simulating streaming)
    let projected_results: Vec<Value> = results
        .into_iter()
        .map(|row| transform_single_row(row, requested_fields, type_name))
        .collect();

    // Wrap in GraphQL data envelope
    let mut data = Map::new();
    data.insert("users".to_string(), Value::Array(projected_results));

    let mut response = Map::new();
    response.insert("data".to_string(), Value::Object(data));

    Value::Object(response)
}

// =============================================================================
// Full Pipeline Benchmarks (10K rows)
// =============================================================================

#[cfg(feature = "postgres")]
fn bench_postgres_full_pipeline_10k(c: &mut Criterion) {
    let Some(conn_str) = get_connection_string() else {
        eprintln!("Skipping PostgresAdapter benchmarks: DATABASE_URL not set");
        return;
    };

    let rt = Runtime::new().unwrap();

    if !rt.block_on(verify_benchmark_data(&conn_str)) {
        eprintln!("Skipping benchmarks: Test data not found in v_benchmark_data");
        return;
    }

    let mut group = c.benchmark_group("full_pipeline_10k");
    group.throughput(Throughput::Elements(10_000));

    group.bench_function(BenchmarkId::new("postgres", "complete"), |b| {
        b.to_async(&rt).iter(|| async {
            let adapter = PostgresAdapter::new(&conn_str).await.unwrap();

            let start = Instant::now();

            // Step 1: Execute database query
            let results = adapter
                .execute_where_query("v_benchmark_data", None, Some(10_000), None)
                .await
                .unwrap();

            // Step 2-5: Transform through full FraiseQL pipeline
            let graphql_response = transform_results(
                results,
                &["id", "name", "email", "status", "created_at"],
                "User",
            );

            black_box(graphql_response);
            black_box(start.elapsed());
        });
    });

    group.finish();
}

#[cfg(feature = "wire-backend")]
fn bench_wire_full_pipeline_10k(c: &mut Criterion) {
    let Some(conn_str) = get_connection_string() else {
        eprintln!("Skipping FraiseWireAdapter benchmarks: DATABASE_URL not set");
        return;
    };

    let rt = Runtime::new().unwrap();

    if !rt.block_on(verify_benchmark_data(&conn_str)) {
        eprintln!("Skipping benchmarks: Test data not found in v_benchmark_data");
        return;
    };

    let mut group = c.benchmark_group("full_pipeline_10k");
    group.throughput(Throughput::Elements(10_000));

    group.bench_function(BenchmarkId::new("wire", "complete"), |b| {
        b.to_async(&rt).iter(|| async {
            let adapter = FraiseWireAdapter::new(&conn_str).with_chunk_size(1024);

            let start = Instant::now();

            // Step 1: Execute database query (streaming)
            let results = adapter
                .execute_where_query("v_benchmark_data", None, Some(10_000), None)
                .await
                .unwrap();

            // Step 2-5: Transform through full FraiseQL pipeline
            let graphql_response = transform_results(
                results,
                &["id", "name", "email", "status", "created_at"],
                "User",
            );

            black_box(graphql_response);
            black_box(start.elapsed());
        });
    });

    group.finish();
}

// =============================================================================
// Full Pipeline Benchmarks (100K rows)
// =============================================================================

#[cfg(feature = "postgres")]
fn bench_postgres_full_pipeline_100k(c: &mut Criterion) {
    let Some(conn_str) = get_connection_string() else {
        return;
    };

    let rt = Runtime::new().unwrap();

    if !rt.block_on(verify_benchmark_data(&conn_str)) {
        return;
    }

    let mut group = c.benchmark_group("full_pipeline_100k");
    group.throughput(Throughput::Elements(100_000));
    group.sample_size(20);

    group.bench_function(BenchmarkId::new("postgres", "complete"), |b| {
        b.to_async(&rt).iter(|| async {
            let adapter = PostgresAdapter::new(&conn_str).await.unwrap();

            let start = Instant::now();
            let results = adapter
                .execute_where_query("v_benchmark_data", None, Some(100_000), None)
                .await
                .unwrap();

            let graphql_response = transform_results(
                results,
                &["id", "name", "email", "status", "created_at"],
                "User",
            );

            black_box(graphql_response);
            black_box(start.elapsed());
        });
    });

    group.finish();
}

#[cfg(feature = "wire-backend")]
fn bench_wire_full_pipeline_100k(c: &mut Criterion) {
    let Some(conn_str) = get_connection_string() else {
        return;
    };

    let rt = Runtime::new().unwrap();

    if !rt.block_on(verify_benchmark_data(&conn_str)) {
        return;
    }

    let mut group = c.benchmark_group("full_pipeline_100k");
    group.throughput(Throughput::Elements(100_000));
    group.sample_size(20);

    group.bench_function(BenchmarkId::new("wire", "complete"), |b| {
        b.to_async(&rt).iter(|| async {
            let adapter = FraiseWireAdapter::new(&conn_str).with_chunk_size(1024);

            let start = Instant::now();
            let results = adapter
                .execute_where_query("v_benchmark_data", None, Some(100_000), None)
                .await
                .unwrap();

            let graphql_response = transform_results(
                results,
                &["id", "name", "email", "status", "created_at"],
                "User",
            );

            black_box(graphql_response);
            black_box(start.elapsed());
        });
    });

    group.finish();
}

// =============================================================================
// Full Pipeline Benchmarks (1M rows) - Memory stress test
// =============================================================================

#[cfg(feature = "postgres")]
fn bench_postgres_full_pipeline_1m(c: &mut Criterion) {
    let Some(conn_str) = get_connection_string() else {
        return;
    };

    let rt = Runtime::new().unwrap();

    if !rt.block_on(verify_benchmark_data(&conn_str)) {
        return;
    }

    let mut group = c.benchmark_group("full_pipeline_1m");
    group.throughput(Throughput::Elements(1_000_000));
    group.sample_size(10);

    group.bench_function(BenchmarkId::new("postgres", "complete"), |b| {
        b.to_async(&rt).iter(|| async {
            let adapter = PostgresAdapter::new(&conn_str).await.unwrap();

            let start = Instant::now();
            let results = adapter
                .execute_where_query("v_benchmark_data", None, Some(1_000_000), None)
                .await
                .unwrap();

            let graphql_response = transform_results(
                results,
                &["id", "name", "email", "status", "created_at"],
                "User",
            );

            black_box(graphql_response);
            black_box(start.elapsed());
        });
    });

    group.finish();
}

#[cfg(feature = "wire-backend")]
fn bench_wire_full_pipeline_1m(c: &mut Criterion) {
    let Some(conn_str) = get_connection_string() else {
        return;
    };

    let rt = Runtime::new().unwrap();

    if !rt.block_on(verify_benchmark_data(&conn_str)) {
        return;
    }

    let mut group = c.benchmark_group("full_pipeline_1m");
    group.throughput(Throughput::Elements(1_000_000));
    group.sample_size(10);

    group.bench_function(BenchmarkId::new("wire", "complete"), |b| {
        b.to_async(&rt).iter(|| async {
            let adapter = FraiseWireAdapter::new(&conn_str).with_chunk_size(2048);

            let start = Instant::now();
            let results = adapter
                .execute_where_query("v_benchmark_data", None, Some(1_000_000), None)
                .await
                .unwrap();

            let graphql_response = transform_results(
                results,
                &["id", "name", "email", "status", "created_at"],
                "User",
            );

            black_box(graphql_response);
            black_box(start.elapsed());
        });
    });

    group.finish();
}

// =============================================================================
// Benchmark Groups
// =============================================================================

#[cfg(feature = "postgres")]
criterion_group!(
    postgres_full_pipeline,
    bench_postgres_full_pipeline_10k,
    bench_postgres_full_pipeline_100k,
    bench_postgres_full_pipeline_1m
);

#[cfg(feature = "wire-backend")]
criterion_group!(
    wire_full_pipeline,
    bench_wire_full_pipeline_10k,
    bench_wire_full_pipeline_100k,
    bench_wire_full_pipeline_1m
);

#[cfg(all(feature = "postgres", feature = "wire-backend"))]
criterion_main!(postgres_full_pipeline, wire_full_pipeline);

#[cfg(all(feature = "postgres", not(feature = "wire-backend")))]
criterion_main!(postgres_full_pipeline);

#[cfg(all(not(feature = "postgres"), feature = "wire-backend"))]
criterion_main!(wire_full_pipeline);

#[cfg(all(not(feature = "postgres"), not(feature = "wire-backend")))]
fn main() {
    eprintln!("No database adapters enabled. Enable 'postgres' and/or 'wire-backend' features.");
}
