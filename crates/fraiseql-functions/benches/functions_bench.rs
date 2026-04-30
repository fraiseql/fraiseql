//! Function runtime performance benchmarks.
//!
//! Benchmarks trigger registry loading, SQL classification (when `host-live`
//! feature is enabled), and WASM engine initialisation (when `runtime-wasm`
//! feature is enabled).

#![allow(clippy::unwrap_used)] // Reason: benchmark setup code, panics acceptable
#![allow(missing_docs)] // Reason: criterion_group!/criterion_main! generate undocumented items

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use fraiseql_functions::{FunctionDefinition, RuntimeType, triggers::registry::TriggerRegistry};

/// Benchmark trigger registry loading from function definitions.
fn bench_registry_load(c: &mut Criterion) {
    let definitions_small = vec![
        FunctionDefinition::new("onCreate", "after:mutation:createUser", RuntimeType::Deno),
        FunctionDefinition::new("validate", "before:mutation:createUser", RuntimeType::Deno),
        FunctionDefinition::new("getUser", "http:GET:/users/:id", RuntimeType::Deno),
    ];

    let definitions_large: Vec<_> = (0..50)
        .map(|i| {
            FunctionDefinition::new(
                &format!("fn_{i}"),
                &format!("after:mutation:entity{i}"),
                RuntimeType::Deno,
            )
        })
        .collect();

    let mut group = c.benchmark_group("registry_load");
    group.bench_function("3_functions", |b| {
        b.iter(|| TriggerRegistry::load_from_definitions(black_box(&definitions_small)).unwrap());
    });
    group.bench_with_input(
        BenchmarkId::new("n_functions", 50),
        &definitions_large,
        |b, defs| {
            b.iter(|| TriggerRegistry::load_from_definitions(black_box(defs)).unwrap());
        },
    );
    group.finish();
}

/// Benchmark SQL classification using sqlparser-rs.
#[cfg(feature = "host-live")]
fn bench_sql_classifier(c: &mut Criterion) {
    use fraiseql_functions::host::live::sql_classifier::classify_sql;

    let statements = [
        ("simple_select", "SELECT id, name FROM users WHERE id = $1"),
        (
            "join_select",
            "SELECT u.id, p.name FROM users u JOIN profiles p ON u.id = p.user_id WHERE u.active = true",
        ),
        (
            "cte",
            "WITH active AS (SELECT id FROM users WHERE active = true) SELECT * FROM active WHERE id = $1",
        ),
        ("insert", "INSERT INTO events (user_id, kind) VALUES ($1, $2)"),
        ("update", "UPDATE users SET name = $1 WHERE id = $2"),
    ];

    let mut group = c.benchmark_group("sql_classifier");
    for (label, sql) in &statements {
        group.bench_with_input(BenchmarkId::from_parameter(label), sql, |b, s| {
            b.iter(|| classify_sql(black_box(s)));
        });
    }
    group.finish();
}

/// Benchmark WASM engine initialisation.
#[cfg(feature = "runtime-wasm")]
fn bench_wasm_engine_init(c: &mut Criterion) {
    use fraiseql_functions::runtime::wasm::{WasmConfig, WasmRuntime};

    c.bench_function("wasm_engine_init", |b| {
        b.iter(|| WasmRuntime::new(black_box(&WasmConfig::default())).unwrap());
    });
}

#[cfg(not(any(feature = "host-live", feature = "runtime-wasm")))]
criterion_group!(benches, bench_registry_load);

#[cfg(all(feature = "host-live", not(feature = "runtime-wasm")))]
criterion_group!(benches, bench_registry_load, bench_sql_classifier);

#[cfg(all(not(feature = "host-live"), feature = "runtime-wasm"))]
criterion_group!(benches, bench_registry_load, bench_wasm_engine_init);

#[cfg(all(feature = "host-live", feature = "runtime-wasm"))]
criterion_group!(benches, bench_registry_load, bench_sql_classifier, bench_wasm_engine_init);

criterion_main!(benches);
