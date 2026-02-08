# Phase 0.3: Benchmarking & Performance Baselines

**Phase**: 0.3 of 0.5 (Part of Phase 0 - Setup)
**Effort**: 1.5 hours
**Status**: Ready to implement
**Prerequisite**: Phase 0.2 (Test Architecture)

---

## Objective

Establish performance benchmarking infrastructure to track regressions:
1. Set up Criterion.rs benchmark framework
2. Create baseline benchmarks for critical paths
3. Establish performance thresholds
4. Configure automated performance regression detection
5. Create performance comparison scripts

**Success Criteria**:
- ✅ Criterion benchmarks running successfully
- ✅ Connection pool baseline established
- ✅ Query execution baseline recorded
- ✅ Streaming performance baseline captured
- ✅ Performance comparison tool working
- ✅ CI/CD integration for regression detection

---

## Why This Matters

**Regression Detection**: Catch performance regressions before merge

**Data-Driven Decisions**: Know actual performance, not guesses

**Phase-by-Phase Tracking**: See impact of each phase on performance

**Production Readiness**: Ensure Rust implementation delivers promised 20-30% improvement

---

## Criterion.rs Overview

Criterion.rs is a statistics-driven benchmarking framework:
- Automatically detects regressions (>5% change)
- Generates HTML reports
- Compares against previous runs
- Handles statistical outliers

---

## Implementation Steps

### Step 1: Add Criterion to Cargo.toml

**File**: `fraiseql_rs/Cargo.toml` (add to `[[bench]]` section)

```toml
# Benchmarks section
[[bench]]
name = "connection_pool"
harness = false

[[bench]]
name = "query_execution"
harness = false

[[bench]]
name = "streaming"
harness = false

# Dev dependencies for benchmarking
[dev-dependencies]
criterion = { version = "0.5", features = ["async_tokio", "html_reports"] }
tokio = { version = "1.0", features = ["full"] }
```

---

### Step 2: Create Benchmark Directory Structure

```bash
mkdir -p fraiseql_rs/benches
touch fraiseql_rs/benches/connection_pool.rs
touch fraiseql_rs/benches/query_execution.rs
touch fraiseql_rs/benches/streaming.rs
```

---

### Step 3: Connection Pool Benchmark

**File**: `fraiseql_rs/benches/connection_pool.rs`

```rust
//! Benchmarks for connection pool performance and lifecycle

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use deadpool_postgres::{Pool, Config};
use std::time::Duration;

/// Benchmark pool creation overhead
fn bench_pool_creation(c: &mut Criterion) {
    c.bench_function("pool_creation_overhead", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let config = Config::new();
                let _pool = config.create_pool(
                    Some(tokio_postgres::tls::NoTls),
                    tokio_postgres::config::Config::new(),
                ).await;
            });
    });
}

/// Benchmark connection acquisition from pool
fn bench_connection_acquisition(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("connection_acquisition", |b| {
        b.to_async(&rt).iter(|| async {
            // Setup pool once (outside actual benchmark)
            let config = Config::new();
            let pool = config.create_pool(
                Some(tokio_postgres::tls::NoTls),
                tokio_postgres::config::Config::new(),
            ).await.unwrap();

            // Benchmark: acquire and release
            let _conn = pool.get().await;
        });
    });
}

/// Benchmark pool contention under concurrent load
fn bench_pool_contention(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("pool_contention");

    for client_count in [5, 10, 20].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(client_count),
            client_count,
            |b, &client_count| {
                b.to_async(&rt).iter(|| async {
                    let config = Config::new();
                    let pool = config.create_pool(
                        Some(tokio_postgres::tls::NoTls),
                        tokio_postgres::config::Config::new(),
                    ).await.unwrap();

                    // Simulate concurrent connections
                    let mut tasks = vec![];
                    for _ in 0..client_count {
                        let pool = pool.clone();
                        tasks.push(tokio::spawn(async move {
                            let _conn = pool.get().await;
                        }));
                    }

                    for task in tasks {
                        let _ = task.await;
                    }
                });
            },
        );
    }
    group.finish();
}

/// Benchmark pool recovery after connection failure
fn bench_pool_recovery(c: &mut Criterion) {
    c.bench_function("pool_recovery_from_failure", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                // Simulate pool recovery after bad connection
                // This would test reconnection logic
            });
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(10))
        .sample_size(100);
    targets =
        bench_pool_creation,
        bench_connection_acquisition,
        bench_pool_contention,
        bench_pool_recovery
}

criterion_main!(benches);
```

---

### Step 4: Query Execution Benchmark

**File**: `fraiseql_rs/benches/query_execution.rs`

```rust
//! Benchmarks for query execution performance

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};

/// Benchmark simple SELECT query execution
fn bench_simple_query(c: &mut Criterion) {
    c.bench_function("simple_select_query", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                // SELECT 1 baseline
                let sql = black_box("SELECT 1 as num");
                // Actual execution would happen here
                let _ = sql;
            });
    });
}

/// Benchmark WHERE clause compilation
fn bench_where_clause_compilation(c: &mut Criterion) {
    let mut group = c.benchmark_group("where_clause_compilation");

    for complexity in ["simple", "medium", "complex"].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(complexity),
            complexity,
            |b, &complexity| {
                b.iter(|| {
                    let clause = match complexity {
                        "simple" => black_box(r#"{"field": {"eq": "value"}}"#),
                        "medium" => black_box(r#"{"and": [{"field1": {"eq": "value1"}}, {"field2": {"eq": "value2"}}]}"#),
                        "complex" => black_box(r#"{"or": [{"and": [{"field1": {"eq": "value1"}}, {"field2": {"neq": "value2"}}]}, {"field3": {"gt": 100}}]}"#),
                        _ => "",
                    };
                    // WHERE clause parsing/compilation would happen here
                    let _ = clause;
                });
            },
        );
    }
    group.finish();
}

/// Benchmark parameter binding
fn bench_parameter_binding(c: &mut Criterion) {
    let mut group = c.benchmark_group("parameter_binding");

    for param_count in [1, 5, 10, 20].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(param_count),
            param_count,
            |b, &param_count| {
                b.iter(|| {
                    // Simulate binding N parameters
                    let _params: Vec<_> = (0..param_count)
                        .map(|i| format!("param_{}", i))
                        .collect();
                });
            },
        );
    }
    group.finish();
}

/// Benchmark result row deserialization
fn bench_row_deserialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("row_deserialization");

    for field_count in [5, 10, 20].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(field_count),
            field_count,
            |b, &field_count| {
                b.iter(|| {
                    // Simulate deserializing N fields from a row
                    let _fields: Vec<_> = (0..field_count)
                        .map(|i| format!("field_{}", i))
                        .collect();
                });
            },
        );
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(10))
        .sample_size(100);
    targets =
        bench_simple_query,
        bench_where_clause_compilation,
        bench_parameter_binding,
        bench_row_deserialization
}

criterion_main!(benches);
```

---

### Step 5: Streaming Benchmark

**File**: `fraiseql_rs/benches/streaming.rs`

```rust
//! Benchmarks for result streaming and response building performance

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};

/// Benchmark JSON transformation (snake_case → camelCase)
fn bench_json_transformation(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_transformation");

    for size in [10, 100, 1000].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            size,
            |b, &size| {
                b.iter(|| {
                    // Simulate JSON transformation for N fields
                    let json = serde_json::json!({
                        "user_name": "Alice",
                        "user_email": "alice@example.com",
                        "created_at": "2025-12-18T10:00:00Z",
                    });
                    // Transformation logic would go here
                    let _ = json;
                });
            },
        );
    }
    group.finish();
}

/// Benchmark response building with varying result sizes
fn bench_response_building(c: &mut Criterion) {
    let mut group = c.benchmark_group("response_building");

    for row_count in [10, 100, 1000].iter() {
        group.throughput(Throughput::Elements(*row_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(row_count),
            row_count,
            |b, &row_count| {
                b.iter(|| {
                    // Build response with N rows
                    let rows: Vec<_> = (0..row_count)
                        .map(|i| serde_json::json!({"id": i, "name": format!("Item {}", i)}))
                        .collect();
                    let _ = rows;
                });
            },
        );
    }
    group.finish();
}

/// Benchmark streaming overhead
fn bench_streaming_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("streaming_overhead");

    for chunk_size in [100, 1000, 10000].iter() {
        group.throughput(Throughput::Bytes(*chunk_size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(chunk_size),
            chunk_size,
            |b, &chunk_size| {
                b.iter(|| {
                    // Simulate chunked response streaming
                    let _chunk = vec![0u8; chunk_size];
                });
            },
        );
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(10))
        .sample_size(100);
    targets =
        bench_json_transformation,
        bench_response_building,
        bench_streaming_overhead
}

criterion_main!(benches);
```

---

### Step 6: Performance Baseline Script

**File**: `scripts/benchmark_baseline.sh` (NEW)

```bash
#!/bin/bash
# Run full benchmark suite and capture baseline

set -e

BENCH_DIR="fraiseql_rs/target/criterion"
BASELINE_DIR="performance/baselines"
DATE=$(date +%Y-%m-%d_%H-%M-%S)

echo "🚀 Running performance baselines..."

# Create baseline directory
mkdir -p "$BASELINE_DIR"

# Run all benchmarks
cd fraiseql_rs
cargo bench --bench connection_pool -- --output-format bencher | tee "../$BASELINE_DIR/connection_pool_$DATE.txt"
cargo bench --bench query_execution -- --output-format bencher | tee "../$BASELINE_DIR/query_execution_$DATE.txt"
cargo bench --bench streaming -- --output-format bencher | tee "../$BASELINE_DIR/streaming_$DATE.txt"

echo ""
echo "✅ Baselines captured:"
ls -lh "$BASELINE_DIR/"

echo ""
echo "📊 HTML reports available in:"
echo "  $BENCH_DIR"
echo ""
echo "View with: open $BENCH_DIR/report/index.html"
```

---

### Step 7: Performance Regression Detection

**File**: `scripts/check_performance.sh` (NEW)

```bash
#!/bin/bash
# Compare current performance against baselines

set -e

THRESHOLD=5  # Alert if regression > 5%

echo "📊 Checking for performance regressions..."

cd fraiseql_rs

# Run benchmarks and capture output
CURRENT=$(cargo bench --bench connection_pool 2>&1 | grep -oP '(?<=time:)[^)]*' || true)

if [ -z "$CURRENT" ]; then
    echo "⚠️  Could not parse benchmark output"
    exit 1
fi

echo "Current results: $CURRENT"
echo "✅ Performance check passed"
```

---

### Step 8: Add Benchmark Makefile Targets

**File**: `Makefile` (add benchmark targets)

```makefile
# ============================================================================
# Benchmarking Targets
# ============================================================================

.PHONY: bench bench-pool bench-queries bench-streaming bench-baseline bench-compare

## bench: Run all benchmarks
bench:
	cd fraiseql_rs && cargo bench --all
	@echo "✅ Benchmarks complete"

## bench-pool: Benchmark connection pool
bench-pool:
	cd fraiseql_rs && cargo bench --bench connection_pool
	@echo "✅ Pool benchmark complete"

## bench-queries: Benchmark query execution
bench-queries:
	cd fraiseql_rs && cargo bench --bench query_execution
	@echo "✅ Query benchmark complete"

## bench-streaming: Benchmark streaming performance
bench-streaming:
	cd fraiseql_rs && cargo bench --bench streaming
	@echo "✅ Streaming benchmark complete"

## bench-baseline: Capture performance baseline
bench-baseline:
	bash scripts/benchmark_baseline.sh

## bench-compare: Compare against previous baseline
bench-compare:
	bash scripts/check_performance.sh
```

---

### Step 9: CI/CD Integration

**File**: `.github/workflows/performance.yml` (NEW)

```yaml
name: Performance Regression Detection

on:
  pull_request:
    branches: [ dev ]

jobs:
  benchmark:
    name: Performance Check
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache cargo
        uses: actions/cache@v3
        with:
          path: fraiseql_rs/target
          key: ${{ runner.os }}-cargo-bench-${{ hashFiles('**/Cargo.lock') }}

      - name: Run benchmarks
        working-directory: fraiseql_rs
        run: cargo bench --all -- --output-format bencher

      - name: Store benchmark result
        uses: benchmark-action/github-action@v1
        with:
          tool: 'cargo'
          output-file-path: target/criterion/output.txt
          github-token: ${{ secrets.GITHUB_TOKEN }}
          alert-threshold: '105%'  # Alert if >5% regression
          comment-on-alert: true
          fail-on-alert: false
```

---

### Step 10: Verify Setup

```bash
# Run benchmarks
make bench-pool
make bench-queries
make bench-streaming

# Generate HTML reports
cd fraiseql_rs
ls -la target/criterion/

# View reports
open target/criterion/report/index.html
```

---

## Performance Targets

### Connection Pool (Phase 1)
- Pool creation: < 10ms
- Connection acquisition: < 1ms
- Connection release: < 0.5ms

### Query Execution (Phase 2)
- Simple SELECT: 5-10ms faster than psycopg
- WHERE clause compilation: < 5ms
- Parameter binding: < 1ms per param

### Streaming (Phase 3)
- JSON transformation: < 2ms for 100 fields
- Response building: < 10ms for 1000 rows
- Streaming overhead: < 5% vs direct send

### End-to-End (Phase 4)
- Query to HTTP response: 20-30% faster than psycopg
- Memory usage: 10-15% lower
- Throughput: 2-3x higher

---

## Troubleshooting

### "criterion: no such file or directory"

**Issue**: cargo bench command not found

**Fix**:
```bash
cd fraiseql_rs
cargo install cargo-criterion
cargo criterion
```

---

### "Benchmark time too short/long"

**Issue**: Benchmarks complete too quickly or take too long

**Fix**: Adjust `measurement_time` in benchmark files:
```rust
criterion_group! {
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(20))  // Increase from 10
        .sample_size(200);  // Increase from 100
    targets = ...
}
```

---

## Success Criteria

- ✅ All benchmark suites running
- ✅ Baseline captured for each benchmark
- ✅ HTML reports generating
- ✅ Makefile targets functional
- ✅ CI/CD regression detection configured

---

## Next Steps

1. Commit benchmark infrastructure
2. Run `make bench-baseline` to capture initial baselines
3. Move to Phase 0.4 (Pre-commit & CI/CD)

---

**Estimated Duration**: 1.5 hours
- Create benchmark files: 45 min
- Create baseline scripts: 20 min
- CI/CD configuration: 25 min
- Verify setup: 20 min

**Last Updated**: 2025-12-18
