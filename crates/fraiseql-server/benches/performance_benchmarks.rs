//! Performance Benchmarks for FraiseQL Server
//!
//! Benchmarks critical paths using Criterion:
//! 1. Request validation performance
//! 2. Metrics collection overhead
//! 3. Logging performance
//! 4. Query parsing performance
//! 5. Performance under concurrent load

use std::sync::{Arc, atomic::Ordering};

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use fraiseql_server::{
    LogLevel, LogMetrics, MetricsCollector, PerformanceMonitor, QueryPerformance, RequestContext,
    RequestValidator, StructuredLogEntry, TraceContext,
};

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
        b.iter(|| validator.validate_query(black_box(queries[0])))
    });

    c.bench_function("query_validation_complex", |b| {
        b.iter(|| validator.validate_query(black_box(queries[3])))
    });
}

/// Benchmark metrics collection performance
fn bench_metrics_collection(c: &mut Criterion) {
    let collector = MetricsCollector::new();

    c.bench_function("metrics_increment_total", |b| {
        b.iter(|| {
            collector.queries_total.fetch_add(black_box(1), Ordering::Relaxed);
        })
    });

    c.bench_function("metrics_increment_multiple", |b| {
        b.iter(|| {
            collector.queries_total.fetch_add(1, Ordering::Relaxed);
            collector.queries_success.fetch_add(1, Ordering::Relaxed);
            collector.queries_duration_us.fetch_add(black_box(5000), Ordering::Relaxed);
            collector.cache_hits.fetch_add(1, Ordering::Relaxed);
        })
    });

    c.bench_function("metrics_load_stats", |b| {
        b.iter(|| {
            collector.queries_total.load(Ordering::Relaxed);
            collector.queries_success.load(Ordering::Relaxed);
            collector.queries_error.load(Ordering::Relaxed);
            collector.queries_duration_us.load(Ordering::Relaxed);
        })
    });
}

/// Benchmark structured logging performance
fn bench_structured_logging(c: &mut Criterion) {
    let context = RequestContext::new()
        .with_operation("GetUser".to_string())
        .with_user_id("user123".to_string());

    let metrics = LogMetrics::new().with_duration_ms(25.5).with_db_queries(2).with_cache_hit(true);

    c.bench_function("log_entry_creation", |b| {
        b.iter(|| {
            StructuredLogEntry::new(
                LogLevel::Info,
                black_box("Query executed successfully".to_string()),
            )
        })
    });

    c.bench_function("log_entry_with_context", |b| {
        b.iter(|| {
            StructuredLogEntry::new(LogLevel::Info, "Query executed".to_string())
                .with_request_context(black_box(context.clone()))
        })
    });

    c.bench_function("log_entry_with_metrics", |b| {
        b.iter(|| {
            StructuredLogEntry::new(LogLevel::Info, "Query executed".to_string())
                .with_metrics(black_box(metrics.clone()))
        })
    });

    c.bench_function("log_entry_json_serialization", |b| {
        let entry = StructuredLogEntry::new(LogLevel::Info, "Test".to_string())
            .with_request_context(context.clone())
            .with_metrics(metrics.clone());

        b.iter(|| entry.to_json_string())
    });
}

/// Benchmark performance monitoring
fn bench_performance_monitoring(c: &mut Criterion) {
    let monitor = PerformanceMonitor::new(100.0);

    c.bench_function("performance_monitor_record", |b| {
        b.iter(|| {
            let perf = QueryPerformance::new(
                black_box(25000),
                black_box(2),
                black_box(5),
                black_box(false),
                black_box(15000),
            );
            monitor.record_query(black_box(perf));
        })
    });

    c.bench_function("performance_monitor_stats", |b| {
        // Pre-populate some data
        for i in 0..100 {
            let perf = QueryPerformance::new(
                (25000 + i * 100) as u64,
                2,
                5,
                i % 3 == 0,
                (15000 + i * 50) as u64,
            );
            monitor.record_query(perf);
        }

        b.iter(|| {
            let _ = monitor.stats();
            let _ = monitor.avg_duration_ms();
            let _ = monitor.slow_query_percentage();
            let _ = monitor.cache_hit_rate();
        })
    });
}

/// Benchmark distributed tracing
fn bench_distributed_tracing(c: &mut Criterion) {
    c.bench_function("trace_context_creation", |b| b.iter(TraceContext::new));

    c.bench_function("trace_context_child_span", |b| {
        let ctx = TraceContext::new();
        b.iter(|| ctx.child_span())
    });

    c.bench_function("trace_context_w3c_header", |b| {
        let ctx = TraceContext::new();
        b.iter(|| ctx.to_w3c_traceparent())
    });

    c.bench_function("trace_context_baggage", |b| {
        b.iter(|| {
            TraceContext::new()
                .with_baggage("user_id".to_string(), black_box("user123".to_string()))
                .with_baggage("tenant".to_string(), black_box("acme".to_string()))
        })
    });
}

/// Benchmark request context creation
fn bench_request_context(c: &mut Criterion) {
    c.bench_function("request_context_new", |b| b.iter(RequestContext::new));

    c.bench_function("request_context_builder", |b| {
        b.iter(|| {
            RequestContext::new()
                .with_operation(black_box("GetUser".to_string()))
                .with_user_id(black_box("user123".to_string()))
                .with_client_ip(black_box("192.168.1.1".to_string()))
        })
    });
}

/// Benchmark concurrent metrics updates
fn bench_concurrent_metrics(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_metrics");

    for num_threads in [2, 4, 8, 16].iter() {
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
                })
            },
        );
    }

    group.finish();
}

/// Benchmark complex log entry
fn bench_complex_log_entry(c: &mut Criterion) {
    let context = RequestContext::new()
        .with_operation("ComplexQuery".to_string())
        .with_user_id("user456".to_string())
        .with_client_ip("10.0.0.1".to_string())
        .with_api_version("v2".to_string());

    let metrics = LogMetrics::new()
        .with_duration_ms(125.5)
        .with_db_queries(5)
        .with_complexity(12)
        .with_cache_hit(false)
        .with_items_processed(1000);

    c.bench_function("complex_log_entry_full", |b| {
        b.iter(|| {
            StructuredLogEntry::new(LogLevel::Warn, "Complex operation completed".to_string())
                .with_request_context(black_box(context.clone()))
                .with_metrics(black_box(metrics.clone()))
                .to_json_string()
        })
    });
}

criterion_group!(
    benches,
    bench_query_validation,
    bench_metrics_collection,
    bench_structured_logging,
    bench_performance_monitoring,
    bench_distributed_tracing,
    bench_request_context,
    bench_concurrent_metrics,
    bench_complex_log_entry
);

criterion_main!(benches);
