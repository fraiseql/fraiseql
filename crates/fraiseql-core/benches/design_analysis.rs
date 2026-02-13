//! Performance benchmarks for design quality analysis
//!
//! Measures:
//! - Rule analysis speed for various schema sizes
//! - Memory usage during analysis
//! - Latency percentiles (p50, p95, p99)

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use fraiseql_core::design::DesignAudit;

// ============================================================================
// Schema Fixtures
// ============================================================================

fn minimal_schema() -> String {
    r#"{
        "types": [
            {"name": "User", "fields": [{"name": "id", "type": "ID", "isPrimaryKey": true}]}
        ]
    }"#
    .to_string()
}

fn typical_schema() -> String {
    r#"{
        "subgraphs": [
            {"name": "users", "entities": ["User"]},
            {"name": "posts", "entities": ["Post"], "references": [{"type": "User"}]},
            {"name": "comments", "entities": ["Comment"], "references": [{"type": "User"}, {"type": "Post"}]}
        ],
        "types": [
            {"name": "User", "fields": [
                {"name": "id", "type": "ID", "isPrimaryKey": true},
                {"name": "name", "type": "String"},
                {"name": "email", "type": "String"}
            ]},
            {"name": "Post", "fields": [
                {"name": "id", "type": "ID", "isPrimaryKey": true},
                {"name": "title", "type": "String"},
                {"name": "content", "type": "String"}
            ]},
            {"name": "Comment", "fields": [
                {"name": "id", "type": "ID", "isPrimaryKey": true},
                {"name": "text", "type": "String"}
            ]}
        ]
    }"#.to_string()
}

fn large_schema() -> String {
    // Schema with many types
    let mut types = vec![];
    for i in 0..100 {
        types.push(format!(
            r#"{{"name": "Type{}", "fields": [{{"name": "id", "type": "ID", "isPrimaryKey": true}}]}}"#,
            i
        ));
    }

    format!(r#"{{"types": [{}]}}"#, types.join(","))
}

fn complex_schema() -> String {
    // Schema with federation and complexity
    r#"{
        "subgraphs": [
            {"name": "a", "entities": ["A"]},
            {"name": "b", "entities": ["B"]},
            {"name": "c", "entities": ["C"]},
            {"name": "d", "entities": ["D"]},
            {"name": "e", "entities": ["E"]}
        ],
        "types": [
            {"name": "Query", "fields": [
                {"name": "allA", "type": "[A!]"},
                {"name": "allB", "type": "[B!]"},
                {"name": "allC", "type": "[C!]"},
                {"name": "allD", "type": "[D!]"},
                {"name": "allE", "type": "[E!]"}
            ]},
            {"name": "A", "fields": [
                {"name": "id", "type": "ID", "isPrimaryKey": true},
                {"name": "b", "type": "[B!]"},
                {"name": "c", "type": "[C!]"}
            ]},
            {"name": "B", "fields": [
                {"name": "id", "type": "ID", "isPrimaryKey": true},
                {"name": "a", "type": "A"},
                {"name": "d", "type": "[D!]"}
            ]},
            {"name": "C", "fields": [
                {"name": "id", "type": "ID", "isPrimaryKey": true},
                {"name": "e", "type": "[E!]"}
            ]},
            {"name": "D", "fields": [
                {"name": "id", "type": "ID", "isPrimaryKey": true}
            ]},
            {"name": "E", "fields": [
                {"name": "id", "type": "ID", "isPrimaryKey": true}
            ]}
        ]
    }"#
    .to_string()
}

// ============================================================================
// Benchmarks
// ============================================================================

fn design_analysis_minimal(c: &mut Criterion) {
    c.bench_function("design_analysis_minimal", |b| {
        b.iter(|| {
            let schema = black_box(minimal_schema());
            DesignAudit::from_schema_json(&schema)
        });
    });
}

fn design_analysis_typical(c: &mut Criterion) {
    c.bench_function("design_analysis_typical", |b| {
        b.iter(|| {
            let schema = black_box(typical_schema());
            DesignAudit::from_schema_json(&schema)
        });
    });
}

fn design_analysis_large(c: &mut Criterion) {
    c.bench_function("design_analysis_large_100_types", |b| {
        b.iter(|| {
            let schema = black_box(large_schema());
            DesignAudit::from_schema_json(&schema)
        });
    });
}

fn design_analysis_complex(c: &mut Criterion) {
    c.bench_function("design_analysis_complex_federation", |b| {
        b.iter(|| {
            let schema = black_box(complex_schema());
            DesignAudit::from_schema_json(&schema)
        });
    });
}

fn design_analysis_suite(c: &mut Criterion) {
    let mut group = c.benchmark_group("design_analysis_suite");

    for schema_type in ["minimal", "typical", "complex"].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(schema_type),
            schema_type,
            |b, &schema_type| {
                b.iter(|| {
                    let schema = match schema_type {
                        "minimal" => black_box(minimal_schema()),
                        "typical" => black_box(typical_schema()),
                        "complex" => black_box(complex_schema()),
                        _ => black_box(minimal_schema()),
                    };
                    DesignAudit::from_schema_json(&schema)
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    design_analysis_minimal,
    design_analysis_typical,
    design_analysis_large,
    design_analysis_complex,
    design_analysis_suite
);

criterion_main!(benches);
