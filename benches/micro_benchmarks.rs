//! Micro-benchmarks for fraiseql-wire core operations
//!
//! These benchmarks measure low-level operations that should be fast:
//! - Protocol encoding/decoding
//! - JSON parsing
//! - Chunking strategy overhead
//! - Error handling
//!
//! Run with: cargo bench --bench micro_benchmarks
//! Run specific benchmark: cargo bench --bench micro_benchmarks protocol_encode

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use serde_json::json;
use fraiseql_wire::connection::ConnectionConfig;

// Test data generators
fn generate_small_json() -> serde_json::Value {
    json!({
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "name": "Test Project",
        "status": "active",
        "priority": 5
    })
}

fn generate_large_json() -> serde_json::Value {
    json!({
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "name": "Test Project",
        "description": "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.",
        "status": "active",
        "priority": 5,
        "team": {
            "id": "team-001",
            "name": "Engineering",
            "members": [
                {"id": "u1", "name": "Alice", "role": "lead"},
                {"id": "u2", "name": "Bob", "role": "senior"},
                {"id": "u3", "name": "Charlie", "role": "junior"},
            ],
            "budget": {
                "total": 150000.0,
                "allocated": 120000.0,
                "remaining": 30000.0,
            }
        },
        "timeline": {
            "start": "2024-01-01T00:00:00Z",
            "estimated_end": "2024-06-30T23:59:59Z",
            "milestones": [
                {"name": "Phase 1", "date": "2024-02-01"},
                {"name": "Phase 2", "date": "2024-03-15"},
                {"name": "Phase 3", "date": "2024-05-01"},
            ]
        },
        "metadata": {
            "created_by": "user-123",
            "created_at": "2024-01-10T10:00:00Z",
            "updated_at": "2024-01-12T15:30:00Z",
            "tags": ["important", "client-facing", "high-visibility"],
            "custom_fields": {
                "external_id": "EXT-12345",
                "business_unit": "Product",
                "cost_center": "ENG-001"
            }
        }
    })
}

fn generate_deeply_nested_json() -> serde_json::Value {
    json!({
        "level1": {
            "level2": {
                "level3": {
                    "level4": {
                        "level5": {
                            "level6": {
                                "level7": {
                                    "level8": {
                                        "data": "deeply nested value",
                                        "count": 42,
                                        "active": true
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

// ============================================================================
// JSON Parsing Benchmarks
// ============================================================================

fn json_parsing_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_parsing");

    // Serialize to string once
    let small_str = serde_json::to_string(&generate_small_json()).unwrap();
    let large_str = serde_json::to_string(&generate_large_json()).unwrap();
    let deep_str = serde_json::to_string(&generate_deeply_nested_json()).unwrap();

    group.bench_with_input(BenchmarkId::from_parameter("small"), &small_str, |b, json_str| {
        b.iter(|| {
            let _: serde_json::Value = serde_json::from_str(black_box(json_str)).unwrap();
        });
    });

    group.bench_with_input(BenchmarkId::from_parameter("large"), &large_str, |b, json_str| {
        b.iter(|| {
            let _: serde_json::Value = serde_json::from_str(black_box(json_str)).unwrap();
        });
    });

    group.bench_with_input(BenchmarkId::from_parameter("deeply_nested"), &deep_str, |b, json_str| {
        b.iter(|| {
            let _: serde_json::Value = serde_json::from_str(black_box(json_str)).unwrap();
        });
    });

    group.finish();
}

// ============================================================================
// Connection String Parsing Benchmarks
// ============================================================================

fn connection_string_parsing_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("connection_parsing");

    let connection_strings = [
        "postgres://localhost/mydb",
        "postgres://user:password@localhost:5432/mydb",
        "postgres://user:pass%40word@localhost:5432/db?application_name=fraiseql-wire",
        "postgres:///mydb",
    ];

    for (idx, conn_str) in connection_strings.iter().enumerate() {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("parse_{}", idx)),
            conn_str,
            |b, &conn_str_ref| {
                b.iter(|| {
                    // Simple parsing logic: extract parts
                    let parts: Vec<&str> = black_box(conn_str_ref).split("://").collect();
                    let _ = parts.len() > 1;
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// BytesMut Chunking Benchmarks
// ============================================================================

fn chunking_strategy_benchmarks(c: &mut Criterion) {
    use bytes::BytesMut;

    let mut group = c.benchmark_group("chunking");

    // Simulate building a chunk with various chunk sizes
    let chunk_sizes = [64, 256, 1024];

    for size in chunk_sizes {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &chunk_size| {
            b.iter(|| {
                let mut buf = BytesMut::with_capacity(black_box(chunk_size));
                for _ in 0..10 {
                    buf.extend_from_slice(b"test_data");
                }
                buf.is_empty()
            });
        });
    }

    group.finish();
}

// ============================================================================
// Error Handling Benchmarks
// ============================================================================

fn error_handling_benchmarks(c: &mut Criterion) {
    use std::io;

    let mut group = c.benchmark_group("error_handling");

    group.bench_function("error_construction", |b| {
        b.iter(|| {
            let _err = io::Error::new(io::ErrorKind::ConnectionRefused, "connection refused");
        });
    });

    group.bench_function("error_conversion_to_string", |b| {
        let err = io::Error::new(io::ErrorKind::TimedOut, "operation timed out");
        b.iter(|| {
            let _s = err.to_string();
        });
    });

    group.finish();
}

// ============================================================================
// String Matching for SQL Predicates
// ============================================================================

fn string_matching_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_matching");

    let test_string = "project__status__name = 'active' AND project__priority__value > 5";

    group.bench_function("contains_check", |b| {
        b.iter(|| {
            black_box(test_string).contains(black_box("project__"))
        });
    });

    group.bench_function("split_operation", |b| {
        b.iter(|| {
            let _: Vec<_> = black_box(test_string).split(black_box("AND")).collect();
        });
    });

    group.finish();
}

// ============================================================================
// ConnectionConfig Creation (Represents actual connection setup)
// ============================================================================

fn connection_config_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("connection_config");

    // Typical minimal configuration
    group.bench_function("minimal_config", |b| {
        b.iter(|| {
            let _config = ConnectionConfig::new(
                black_box("fraiseql_test"),
                black_box("postgres"),
            );
        });
    });

    // Typical configuration with password and parameters
    group.bench_function("full_config_with_params", |b| {
        b.iter(|| {
            let _config = ConnectionConfig::new(
                black_box("fraiseql_test"),
                black_box("postgres"),
            )
            .password(black_box("secret_password"))
            .param(black_box("application_name"), black_box("fraiseql-wire"))
            .param(black_box("statement_timeout"), black_box("30000"))
            .param(black_box("connect_timeout"), black_box("10"));
        });
    });

    // Complex configuration with many parameters
    group.bench_function("complex_config_many_params", |b| {
        b.iter(|| {
            let _config = ConnectionConfig::new(
                black_box("fraiseql_test"),
                black_box("postgres"),
            )
            .password(black_box("secret_password"))
            .param(black_box("application_name"), black_box("fraiseql-wire"))
            .param(black_box("statement_timeout"), black_box("30000"))
            .param(black_box("connect_timeout"), black_box("10"))
            .param(black_box("keepalives"), black_box("1"))
            .param(black_box("keepalives_idle"), black_box("30"))
            .param(black_box("keepalives_interval"), black_box("10"));
        });
    });

    group.finish();
}

// ============================================================================
// Connection String Parsing by Protocol (TCP vs Unix Socket)
// ============================================================================

fn connection_protocol_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("connection_protocol");

    // TCP connection string parsing
    group.bench_function("parse_tcp_localhost", |b| {
        b.iter(|| {
            let conn_str = black_box("postgres://localhost:5432/fraiseql_test");
            let parts: Vec<&str> = conn_str.split("://").collect();
            let _ = parts.len() > 1;
        });
    });

    // TCP with full credentials
    group.bench_function("parse_tcp_with_credentials", |b| {
        b.iter(|| {
            let conn_str = black_box("postgres://user:password@localhost:5432/fraiseql_test");
            let parts: Vec<&str> = conn_str.split("://").collect();
            let _ = parts.len() > 1;
        });
    });

    // Unix socket connection string parsing
    group.bench_function("parse_unix_socket", |b| {
        b.iter(|| {
            let conn_str = black_box("postgres:///fraiseql_test");
            let parts: Vec<&str> = conn_str.split("://").collect();
            let _ = parts.len() > 1;
        });
    });

    // Unix socket with socket directory
    group.bench_function("parse_unix_socket_custom_dir", |b| {
        b.iter(|| {
            let conn_str = black_box("postgres:///fraiseql_test?host=/var/run/postgresql");
            let parts: Vec<&str> = conn_str.split("://").collect();
            let _ = parts.len() > 1;
        });
    });

    group.finish();
}

// ============================================================================
// Metrics Overhead Benchmarks
// ============================================================================

fn metrics_overhead_benchmarks(c: &mut Criterion) {
    use fraiseql_wire::metrics;

    let mut group = c.benchmark_group("metrics_overhead");

    // Measure counter increment overhead
    group.bench_function("counter_increment", |b| {
        b.iter(|| {
            metrics::counters::query_submitted(
                black_box("users"),
                black_box(true),
                black_box(false),
                black_box(false),
            );
        });
    });

    // Measure histogram recording overhead
    group.bench_function("histogram_record", |b| {
        b.iter(|| {
            metrics::histograms::query_startup_duration(black_box("users"), black_box(100));
        });
    });

    // Measure query completion counter
    group.bench_function("query_completion_counter", |b| {
        b.iter(|| {
            metrics::counters::query_completed(black_box("success"), black_box("users"));
        });
    });

    // Measure authentication metrics
    group.bench_function("auth_metrics_full_path", |b| {
        b.iter(|| {
            metrics::counters::auth_attempted(black_box("scram"));
            metrics::histograms::auth_duration(black_box("scram"), black_box(125));
            metrics::counters::auth_successful(black_box("scram"));
        });
    });

    // Measure deserialization metrics
    group.bench_function("deserialization_success", |b| {
        b.iter(|| {
            metrics::counters::deserialization_success(black_box("users"), black_box("User"));
            metrics::histograms::deserialization_duration(
                black_box("users"),
                black_box("User"),
                black_box(12),
            );
        });
    });

    // Measure filter metrics
    group.bench_function("filter_metrics", |b| {
        b.iter(|| {
            metrics::histograms::filter_duration(black_box("users"), black_box(5));
            metrics::counters::rows_filtered(black_box("users"), black_box(1));
        });
    });

    // Measure chunk processing metrics
    group.bench_function("chunk_metrics_full", |b| {
        b.iter(|| {
            metrics::histograms::chunk_size(black_box("users"), black_box(256));
            metrics::histograms::chunk_processing_duration(black_box("users"), black_box(20));
        });
    });

    // Measure error tracking
    group.bench_function("error_tracking", |b| {
        b.iter(|| {
            metrics::counters::query_error(black_box("users"), black_box("json_parse_error"));
            metrics::counters::json_parse_error(black_box("users"));
        });
    });

    // Typical query pipeline: complete instrumentation
    group.bench_function("complete_query_instrumentation", |b| {
        b.iter(|| {
            // Query submission
            metrics::counters::query_submitted(black_box("users"), true, false, false);
            // Auth
            metrics::counters::auth_attempted(black_box("scram"));
            metrics::histograms::auth_duration(black_box("scram"), 100);
            metrics::counters::auth_successful(black_box("scram"));
            // Startup
            metrics::histograms::query_startup_duration(black_box("users"), 50);
            // Chunk processing (simulate 5 chunks)
            for _ in 0..5 {
                metrics::histograms::chunk_size(black_box("users"), 256);
                metrics::histograms::chunk_processing_duration(black_box("users"), 20);
            }
            // Filter (10% filtered)
            for _ in 0..1280 {
                metrics::histograms::filter_duration(black_box("users"), 2);
            }
            metrics::counters::rows_filtered(black_box("users"), 128);
            // Deserialization
            metrics::counters::deserialization_success(black_box("users"), black_box("User"));
            metrics::histograms::deserialization_duration(black_box("users"), black_box("User"), 12);
            // Completion
            metrics::counters::rows_processed(black_box("users"), 1152, black_box("ok"));
            metrics::histograms::query_total_duration(black_box("users"), 180);
            metrics::counters::query_completed(black_box("success"), black_box("users"));
        });
    });

    group.finish();
}

// ============================================================================
// Criterion Groups and Main
// ============================================================================

criterion_group!(
    benches,
    json_parsing_benchmarks,
    connection_string_parsing_benchmarks,
    chunking_strategy_benchmarks,
    error_handling_benchmarks,
    string_matching_benchmarks,
    connection_config_benchmarks,
    connection_protocol_benchmarks,
    metrics_overhead_benchmarks,
);

criterion_main!(benches);
