# FraiseQL Performance Profiling & Optimization Guide

This guide covers performance profiling, benchmarking, and optimization techniques for FraiseQL.

## Table of Contents

1. [Quick Start](#quick-start)
2. [Profiling Tools](#profiling-tools)
3. [Benchmarking](#benchmarking)
4. [Common Bottlenecks](#common-bottlenecks)
5. [Optimization Techniques](#optimization-techniques)
6. [Performance Monitoring](#performance-monitoring)

## Quick Start

### Fast Profiling

```bash
# Profile a specific test
CARGO_PROFILE_TEST_DEBUG=true cargo flamegraph --test query_execution

# View flamegraph
open flamegraph.svg
```

### Quick Benchmark

```bash
# Run benchmarks
cargo bench

# Compare to baseline
cargo bench -- --save-baseline main
cargo bench -- --baseline main
```

### Check for Regressions

```bash
# Build release and profile
cargo build --release
time ./target/release/fraiseql-cli compile schema.json
```

## Profiling Tools

### 1. Flamegraph (Most Useful)

Visualize where time is spent:

```bash
# Install flamegraph
cargo install flamegraph

# Profile a binary
cargo flamegraph

# Profile a test
cargo flamegraph --test query_execution

# With specific filter
cargo flamegraph -- --test test_complex_query
```

**Reading flamegraphs**:

- Y-axis: Function call stack (bottom = main)
- X-axis: Time spent in function
- Width: Time spent (wider = slower)
- Color: Random (just for visual distinction)

### 2. Perf (Linux)

Low-level CPU profiling:

```bash
# Install perf
sudo apt-get install linux-tools

# Profile binary
perf record -g ./target/release/fraiseql-server

# Generate report
perf report

# Convert to flamegraph
cargo install flamegraph
perf script | stackcollapse-perf.pl | flamegraph.pl > out.svg
```

### 3. Cargo Criterion

Statistically robust benchmarking:

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_query_execution(c: &mut Criterion) {
    c.bench_function("simple_query", |b| {
        b.iter(|| {
            // Code to benchmark
            black_box(execute_query("{ users { id } }"))
        });
    });
}

criterion_group!(benches, benchmark_query_execution);
criterion_main!(benches);
```

Run with:

```bash
cargo bench
```

### 4. Perf Stat

High-level statistics:

```bash
# Count CPU cycles, cache misses, etc.
perf stat ./target/release/fraiseql-server --run-test-query

# Output:
# Performance counter stats for './target/release/fraiseql-server':
#      123,456,789 cycles
#       10,234,567 instructions
#          123,456 L1-dcache-misses
```

## Benchmarking

### Built-in Benchmarks

Run all benchmarks:

```bash
cargo bench

# Filter benchmarks
cargo bench query

# Run specific benchmark
cargo bench -- --exact benchmark_name

# Compare against baseline
cargo bench -- --baseline main
```

### Writing Benchmarks

Create `benches/mytest.rs`:

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use fraiseql_core::schema::CompiledSchema;

fn benchmark_schema_compilation(c: &mut Criterion) {
    let schema_json = include_str!("../fixtures/complex_schema.json");

    c.bench_function("compile_schema", |b| {
        b.iter(|| {
            CompiledSchema::from_json(black_box(schema_json))
        });
    });
}

fn benchmark_query_execution(c: &mut Criterion) {
    let mut group = c.benchmark_group("query_execution");

    let scenarios = vec![
        ("simple", "{ users { id } }"),
        ("medium", "{ users { id posts { title } } }"),
        ("complex", "{ users { id posts { title comments { text } } } }"),
    ];

    for (name, query) in scenarios {
        group.bench_function(name, |b| {
            b.iter(|| execute_query(black_box(query)))
        });
    }
}

criterion_group!(benches, benchmark_schema_compilation, benchmark_query_execution);
criterion_main!(benches);
```

Add to `Cargo.toml`:

```toml
[[bench]]
name = "mytest"
harness = false
```

## Common Bottlenecks

### 1. Schema Parsing

**Symptoms**: Slow startup time, high CPU on init

```bash
# Profile schema compilation
cargo flamegraph --test compile_schema

# Optimize: Use compilation cache
CompiledSchema::from_json_cached(json)
```

**Solutions**:

- Use compilation cache (enabled by default)
- Pre-compile schemas at build time
- Implement lazy loading for large schemas

### 2. Query Matching

**Symptoms**: High latency for complex queries, CPU spike at start of query

```bash
# Profile query matching
cargo flamegraph -- --test benchmark_query_matching
```

**Solutions**:

- Cache query plans (enabled by default)
- Simplify field selection
- Use field-level caching

### 3. Database Queries

**Symptoms**: Request latency increases with result size

```bash
# Enable query tracing
RUST_LOG=fraiseql_core=debug cargo test

# Check output for slow SQL queries
```

**Solutions**:

- Add database indexes
- Use connection pooling
- Batch requests
- Enable query result caching

### 4. Serialization

**Symptoms**: High latency on large result sets

```bash
# Profile JSON serialization
cargo flamegraph -- --test serialize_large_response
```

**Solutions**:

- Stream JSON responses
- Use more efficient serialization
- Implement response compression

### 5. Memory Allocation

**Symptoms**: High memory usage, garbage collection pauses (Rust uses TOCTOU)

```bash
# Check memory usage
/usr/bin/time -v ./target/release/fraiseql-server

# Profile allocations
cargo install valgrind
valgrind --leak-check=full ./target/release/fraiseql-server
```

**Solutions**:

- Use `Vec::with_capacity()` for pre-allocated vectors
- Reuse buffers
- Avoid unnecessary clones

## Optimization Techniques

### 1. Use Criterion Benchmarks

```rust
// ❌ Wrong: Microbenchmark
#[test]
fn bench_function() {
    for _ in 0..1000000 {
        function_under_test();
    }
}

// ✅ Right: Statistical benchmarking
fn bench_function(c: &mut Criterion) {
    c.bench_function("function", |b| b.iter(|| function_under_test()));
}
```

### 2. Use Black Box

Prevent compiler optimizations from skewing results:

```rust
use criterion::black_box;

c.bench_function("algorithm", |b| {
    b.iter(|| {
        let input = black_box(vec![1, 2, 3, 4, 5]);
        sort_algorithm(&input)
    });
});
```

### 3. Cache Hot Data

```rust
// ❌ Recompute every time
fn process_query(query: &str) -> Result<String> {
    let plan = compile_query(query)?;  // Recompile each time
    execute_plan(&plan)
}

// ✅ Cache compiled plans
let cache = Arc::new(Mutex::new(HashMap::new()));
fn process_query(query: &str, cache: &Arc<Mutex<HashMap<String, Plan>>>) -> Result<String> {
    let plan = {
        let mut c = cache.lock().unwrap();
        c.entry(query.to_string())
            .or_insert_with(|| compile_query(query).unwrap())
            .clone()
    };
    execute_plan(&plan)
}
```

### 4. Reduce Allocations

```rust
// ❌ Creates new string each iteration
let mut result = String::new();
for item in items {
    result = format!("{},{}", result, item);  // Allocates new String each time
}

// ✅ Use push_str to reuse buffer
let mut result = String::new();
for (i, item) in items.iter().enumerate() {
    if i > 0 { result.push(','); }
    result.push_str(&item.to_string());
}

// ✅ Or use join
let result = items.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
```

### 5. Specialize Hot Paths

```rust
// ❌ Generic, works for all types but slower
fn process<T>(items: Vec<T>) -> Vec<T> {
    // Complex generic code
}

// ✅ Specialize for common case
fn process_fast(items: Vec<u32>) -> Vec<u32> {
    // Optimized for u32, uses CPU-specific instructions
}

fn process<T>(items: Vec<T>) -> Vec<T> {
    // Generic fallback
}
```

## Performance Monitoring

### 1. Query Tracing

FraiseQL provides built-in query execution traces:

```rust
use fraiseql_core::runtime::query_tracing::QueryTraceBuilder;

let mut trace = QueryTraceBuilder::new("query_123", "{ users { id } }");

// Record phases
let phase_start = Instant::now();
execute_phase();
trace.record_phase_success("execute", phase_start.elapsed().as_micros() as u64);

// Get metrics
let finished = trace.finish(true, None, Some(100))?;
println!("Query took {} us", finished.total_duration_us);
println!("Slowest phase: {:?}", finished.slowest_phase());
```

### 2. Metrics Collection

Monitor server metrics:

```bash
# Prometheus metrics available at /metrics
curl http://localhost:8000/metrics

# JSON metrics at /metrics/json
curl http://localhost:8000/metrics/json | jq .query_count
```

### 3. Logging Performance Data

```bash
# Enable detailed logging
RUST_LOG=fraiseql_core=debug cargo run

# Filter specific module
RUST_LOG=fraiseql_core::runtime::executor=debug cargo run

# All modules with trace level
RUST_LOG=trace cargo run
```

### 4. APM Integration

FraiseQL supports OpenTelemetry:

```bash
# With Jaeger for distributed tracing
docker run -d -p 16686:16686 jaegertracing/all-in-one

# Configure in config.toml
[tracing]
enabled = true
otel_exporter_endpoint = "http://localhost:4317"
```

## Performance Checklist

Before optimizing:

- [ ] Have you profiled the code?
- [ ] Is the bottleneck in your code or dependencies?
- [ ] Have you run benchmarks before/after?
- [ ] Did you enable release mode (`--release`)?
- [ ] Have you considered caching?
- [ ] Is memory usage acceptable?

Don't optimize:

- [ ] Premature optimization (profile first!)
- [ ] Readable code (readability > microbenchmarks)
- [ ] Rare code paths (focus on hot paths)
- [ ] Without data (benchmark everything)

## Resources

- [Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [Criterion.rs Guide](https://bheisler.github.io/criterion.rs/book/criterion_rs.html)
- [Linux Perf Examples](https://www.brendangregg.com/perf.html)
- [Flamegraph Guide](https://www.brendangregg.com/flamegraphs.html)

## Examples

### Profile a Specific Benchmark

```bash
# Run a specific benchmark with profiling
cargo bench --bench performance_benchmarks -- --profile-time=10

# Profile only one scenario
cargo bench --bench performance_benchmarks -- cache::hit --profile-time=10
```

### Compare Two Versions

```bash
# Save baseline on main branch
git checkout main
cargo bench -- --save-baseline main

# Test on feature branch
git checkout feature/optimization
cargo bench -- --baseline main

# Results show % improvement/regression
```

### Optimize Query Compilation

```bash
# Profile compilation
time cargo build --release

# Add to .cargo/config.toml
[build]
jobs = 8  # Parallel jobs
```

Happy optimizing! 🚀
