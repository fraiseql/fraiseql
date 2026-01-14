//! Phase 6 Validation Benchmarks
//!
//! Measures the impact of Phase 6 (lazy pause/resume initialization)
//! on real Postgres queries.
//!
//! This benchmark validates that Phase 6's lazy initialization of pause/resume
//! infrastructure reduces startup overhead by ~5-8ms per query.
//!
//! Run with: cargo bench --bench phase6_validation --features bench-with-postgres
//!
//! Expected results:
//! - Small result sets (1K-50K rows): Improved by 5-8ms per iteration
//! - Time-to-first-row should show measurable improvement
//! - Throughput unchanged (lazy init only affects startup)

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use fraiseql_wire::client::FraiseClient;
use futures::StreamExt;
use serde_json::Value;
use std::time::Instant;

/// Benchmark a streaming query with a given size
/// Creates a fresh connection and runtime for each iteration
async fn benchmark_query(size: usize) -> Result<u128, Box<dyn std::error::Error>> {
    let client = FraiseClient::connect("postgres://postgres@localhost/fraiseql_bench").await?;

    let start = Instant::now();

    // Query the v_test_1m view (1 million rows with typical JSON structure)
    // This exercises the full streaming pipeline including Phase 6's lazy initialization
    let mut stream = client
        .query::<Value>("v_test_1m")
        .limit(black_box(size))
        .execute()
        .await?;

    let mut count = 0;
    while let Some(_item) = stream.next().await {
        count += 1;
    }

    let elapsed = start.elapsed().as_micros();
    black_box(count);

    Ok(elapsed)
}

fn small_result_sets(c: &mut Criterion) {
    let mut group = c.benchmark_group("phase6_small_sets");

    // Use a fresh runtime for each benchmark function
    // This is necessary because FraiseClient requires exclusive connection access
    let rt = tokio::runtime::Runtime::new().unwrap();

    // 1K rows
    group.throughput(Throughput::Elements(1000));
    group.bench_function("1k_rows", |b| {
        b.to_async(&rt).iter(|| async {
            benchmark_query(1_000).await.unwrap()
        });
    });

    // 10K rows (critical measurement - this is where the 23.5% gap was)
    group.throughput(Throughput::Elements(10000));
    group.bench_function("10k_rows", |b| {
        b.to_async(&rt).iter(|| async {
            benchmark_query(10_000).await.unwrap()
        });
    });

    // 50K rows
    group.throughput(Throughput::Elements(50000));
    group.bench_function("50k_rows", |b| {
        b.to_async(&rt).iter(|| async {
            benchmark_query(50_000).await.unwrap()
        });
    });

    group.finish();
}

fn large_result_sets(c: &mut Criterion) {
    let mut group = c.benchmark_group("phase6_large_sets");
    group.measurement_time(std::time::Duration::from_secs(15));
    group.sample_size(10);

    let rt = tokio::runtime::Runtime::new().unwrap();

    // 100K rows (where Phase 6 impact is less noticeable)
    group.throughput(Throughput::Elements(100000));
    group.bench_function("100k_rows", |b| {
        b.to_async(&rt).iter(|| async {
            benchmark_query(100_000).await.unwrap()
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    small_result_sets,
    large_result_sets
);
criterion_main!(benches);
