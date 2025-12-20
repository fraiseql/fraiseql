//! Benchmarks for unified Rust pipeline (Phase 9).

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::collections::HashMap;
use serde_json::Value as JsonValue;

/// Mock user context for benchmarks
fn create_test_user() -> fraiseql_rs::pipeline::unified::UserContext {
    fraiseql_rs::pipeline::unified::UserContext {
        user_id: Some("test_user".to_string()),
        permissions: vec!["read".to_string()],
        roles: vec!["user".to_string()],
    }
}

/// Mock schema for benchmarks
fn create_test_schema() -> String {
    r#"{
        "tables": {
            "users": {
                "view_name": "v_users",
                "sql_columns": ["id", "email", "status"],
                "jsonb_column": "data",
                "fk_mappings": {},
                "has_jsonb_data": true
            }
        },
        "types": {}
    }"#.to_string()
}

fn benchmark_simple_query(c: &mut Criterion) {
    c.bench_function("phase9_simple_query", |b| {
        // Initialize pipeline
        let schema_json = create_test_schema();
        fraiseql_rs::initialize_graphql_pipeline(schema_json).unwrap();

        b.to_async(tokio::runtime::Runtime::new().unwrap()).iter(|| async {
            let result = fraiseql_rs::execute_graphql_query(
                black_box("query { users { id } }".to_string()),
                black_box(pyo3::types::PyDict::new(pyo3::Python::acquire_gil().python())),
                black_box(pyo3::types::PyDict::new(pyo3::Python::acquire_gil().python())),
            ).await;
            black_box(result)
        });
    });
}

fn benchmark_complex_where(c: &mut Criterion) {
    c.bench_function("phase9_complex_where", |b| {
        // Initialize pipeline
        let schema_json = create_test_schema();
        fraiseql_rs::initialize_graphql_pipeline(schema_json).unwrap();

        let query = r#"
            query {
                users(where: {
                    AND: [
                        {status: "active"},
                        {email: {like: "test"}}
                    ]
                }) {
                    id
                    email
                    status
                }
            }
        "#;

        b.to_async(tokio::runtime::Runtime::new().unwrap()).iter(|| async {
            let result = fraiseql_rs::execute_graphql_query(
                black_box(query.to_string()),
                black_box(pyo3::types::PyDict::new(pyo3::Python::acquire_gil().python())),
                black_box(pyo3::types::PyDict::new(pyo3::Python::acquire_gil().python())),
            ).await;
            black_box(result)
        });
    });
}

fn benchmark_cached_query(c: &mut Criterion) {
    c.bench_function("phase9_cached_query", |b| {
        // Initialize pipeline
        let schema_json = create_test_schema();
        fraiseql_rs::initialize_graphql_pipeline(schema_json).unwrap();

        let query = "query { users { id } }";

        b.to_async(tokio::runtime::Runtime::new().unwrap()).iter(|| async {
            // Run query twice to test cache on second call
            let _ = fraiseql_rs::execute_graphql_query(
                black_box(query.to_string()),
                black_box(pyo3::types::PyDict::new(pyo3::Python::acquire_gil().python())),
                black_box(pyo3::types::PyDict::new(pyo3::Python::acquire_gil().python())),
            ).await;

            let result = fraiseql_rs::execute_graphql_query(
                black_box(query.to_string()),
                black_box(pyo3::types::PyDict::new(pyo3::Python::acquire_gil().python())),
                black_box(pyo3::types::PyDict::new(pyo3::Python::acquire_gil().python())),
            ).await;
            black_box(result)
        });
    });
}

criterion_group!(
    benches,
    benchmark_simple_query,
    benchmark_complex_where,
    benchmark_cached_query
);
criterion_main!(benches);