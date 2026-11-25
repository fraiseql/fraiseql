//! Performance Benchmarks for FraiseQL Server
//!
//! Benchmarks critical paths using Criterion:
//! 1. Request validation performance
//! 2. Metrics collection overhead
//! 3. Metrics updates under concurrent load

#![allow(clippy::unwrap_used)] // Reason: benchmark setup code, panics acceptable
#![allow(missing_docs)] // Reason: criterion_group!/criterion_main! macros generate undocumented items
#![allow(clippy::cast_precision_loss)] // Reason: bench reporting uses integer→f64 for human-readable output
#![allow(clippy::cast_sign_loss)] // Reason: bench data uses small positive integers
#![allow(clippy::cast_possible_truncation)] // Reason: bench data values are small and bounded

use std::sync::{Arc, atomic::Ordering};

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use fraiseql_server::{MetricsCollector, RequestValidator};

/// Benchmark query validation performance
fn bench_query_validation(c: &mut Criterion) {
    let validator = RequestValidator::new();

    let queries = [
        "{ user { id } }",
        "{ users { id name email } }",
        "query { post { title body } }",
        "{ user { profile { settings { theme } } } }",
    ];

    c.bench_function("query_validation_simple", |b| {
        b.iter(|| validator.validate_query(black_box(queries[0])));
    });

    c.bench_function("query_validation_complex", |b| {
        b.iter(|| validator.validate_query(black_box(queries[3])));
    });
}

/// Benchmark metrics collection performance
fn bench_metrics_collection(c: &mut Criterion) {
    let collector = MetricsCollector::new();

    c.bench_function("metrics_increment_total", |b| {
        b.iter(|| {
            collector.queries_total.fetch_add(black_box(1), Ordering::Relaxed);
        });
    });

    c.bench_function("metrics_increment_multiple", |b| {
        b.iter(|| {
            collector.queries_total.fetch_add(1, Ordering::Relaxed);
            collector.queries_success.fetch_add(1, Ordering::Relaxed);
            collector.queries_duration_us.fetch_add(black_box(5000), Ordering::Relaxed);
        });
    });

    c.bench_function("metrics_load_stats", |b| {
        b.iter(|| {
            collector.queries_total.load(Ordering::Relaxed);
            collector.queries_success.load(Ordering::Relaxed);
            collector.queries_error.load(Ordering::Relaxed);
            collector.queries_duration_us.load(Ordering::Relaxed);
        });
    });
}

/// Benchmark concurrent metrics updates
fn bench_concurrent_metrics(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_metrics");

    for num_threads in &[2, 4, 8, 16] {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_threads),
            num_threads,
            |b, &num_threads| {
                let collector = Arc::new(MetricsCollector::new());

                b.to_async(tokio::runtime::Runtime::new().unwrap()).iter(|| async {
                    let mut handles = vec![];

                    for _ in 0..num_threads {
                        let collector = collector.clone();
                        let handle = tokio::spawn(async move {
                            for _ in 0..100 {
                                collector.queries_total.fetch_add(1, Ordering::Relaxed);
                            }
                        });
                        handles.push(handle);
                    }

                    for handle in handles {
                        let _ = handle.await;
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_query_validation,
    bench_metrics_collection,
    bench_concurrent_metrics
);

criterion_main!(benches);
