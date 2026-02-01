# FraiseQL Benchmarking Guide

This guide covers setting up and running FraiseQL's comprehensive performance benchmarking infrastructure.

## Quick Start

### 1. One-Command Setup

```bash
cd /home/lionel/code/fraiseql
bash BENCHMARK_QUICK_START.sh setup
```

This will:
- Start Docker Compose with PostgreSQL, MySQL, SQLite, and SQL Server
- Wait for services to be healthy
- Load benchmark dataset
- Verify database connectivity

### 2. Run Benchmarks

```bash
# Run small dataset (10K rows, ~2 minutes)
bash BENCHMARK_QUICK_START.sh run-small

# Run medium dataset (100K rows, ~5 minutes)
bash BENCHMARK_QUICK_START.sh run-medium

# Run large dataset (1M rows, ~10 minutes)
bash BENCHMARK_QUICK_START.sh run-large

# Run all benchmarks (full suite, ~15-30 minutes)
bash BENCHMARK_QUICK_START.sh run-all

# Run fraiseql-wire micro benchmarks only
bash BENCHMARK_QUICK_START.sh wire-micro
```

### 3. View Results

```bash
bash BENCHMARK_QUICK_START.sh report
```

Opens HTML report with statistical analysis, charts, and regression detection.

### 4. Clean Up

```bash
bash BENCHMARK_QUICK_START.sh clean
```

Stops Docker containers and cleans up benchmark environment.

## Benchmark Overview

### Total Benchmarks: 11 files

#### fraiseql-core Benchmarks (5 benchmarks)

Located: `crates/fraiseql-core/benches/`

| File | Purpose | Status |
|------|---------|--------|
| `adapter_comparison.rs` | PostgreSQL vs FraiseWire adapter performance | ✅ Ready |
| `federation_bench.rs` | Federation query performance | ✅ Ready |
| `full_pipeline_comparison.rs` | Complete GraphQL execution pipeline | ✅ Ready |
| `saga_performance_bench.rs` | Distributed transaction performance | ✅ Ready |
| `sql_projection_benchmark.rs` | Field projection efficiency | ✅ Ready |

#### fraiseql-wire Benchmarks (4 benchmarks)

Located: `crates/fraiseql-wire/benches/`

| File | Purpose | Status |
|------|---------|--------|
| `micro_benchmarks.rs` | JSON parsing, chunking, error handling | ✅ Ready |
| `integration_benchmarks.rs` | PostgreSQL integration performance | ✅ Ready |
| `comparison_benchmarks.rs` | Wire protocol comparisons | ✅ Ready |
| `phase6_validation.rs` | Phase 6 validation benchmarks | ✅ Ready |

#### fraiseql-server Benchmarks (1 benchmark)

Located: `crates/fraiseql-server/benches/`

| File | Purpose | Status |
|------|---------|--------|
| `performance_benchmarks.rs` | HTTP server performance | ✅ Ready |

#### Workspace Benchmarks (1 benchmark)

Located: `benches/`

| File | Purpose | Status |
|------|---------|--------|
| `arrow_flight_benchmarks.rs` | Arrow Flight performance | ✅ Ready |

## Benchmark Infrastructure

### Criterion Configuration

Status: ✅ Configured in all crates

```toml
[dev-dependencies]
criterion = { version = "0.5", features = ["async_tokio"] }
tokio-test = "0.4"
```

**Features Enabled:**
- `async_tokio` — Async benchmark support
- Statistical analysis
- HTML report generation to `target/criterion/`
- Regression detection
- Trend analysis

### Database Support

**Primary:** PostgreSQL
- Used for most benchmarks
- Full feature coverage
- Connection pooling enabled

**Secondary:** MySQL, SQLite, SQL Server
- Performance comparison benchmarks
- Adapter-specific optimizations

### Running Individual Benchmarks

#### Core Pipeline Comparison

Measures complete GraphQL execution from parse to result projection:

```bash
cargo bench --bench full_pipeline_comparison --features "postgres,wire-backend"
```

Metrics:
- Parse time
- Authorization check time
- Query planning time
- Database execution time
- Result projection time

#### Adapter Comparison

PostgreSQL adapter vs FraiseWire adapter head-to-head:

```bash
cargo bench --bench adapter_comparison --features "postgres,wire-backend"
```

Datasets: 10K rows, 100K rows, 1M rows

#### Federation Performance

Multi-schema query composition performance:

```bash
cargo bench --bench federation_bench
```

Measures:
- Cross-schema query planning
- Result merging overhead
- Authorization enforcement

#### Wire Micro Benchmarks

Low-level protocol performance:

```bash
cargo bench --bench micro_benchmarks -p fraiseql-wire
```

Measures microsecond-scale operations:
- JSON parsing
- Frame chunking
- Error handling

#### Saga Performance

Distributed transaction coordinator performance:

```bash
cargo bench --bench saga_performance_bench
```

Measures:
- State machine transitions
- Saga recovery
- Concurrent operation handling

### Interpreting Results

#### HTML Reports

After running benchmarks, view detailed analysis:

```bash
open target/criterion/report/index.html
```

Reports include:
- Performance graphs with confidence intervals
- Regression detection (comparing to baseline)
- Throughput analysis
- Variance statistics

#### Regression Detection

Criterion automatically detects performance regressions:

- **Green**: Performance improved or stable
- **Orange**: Performance regressed (within threshold)
- **Red**: Performance regressed significantly

Compare against baseline by running same benchmark twice:

```bash
cargo bench --bench adapter_comparison -- --verbose
```

### Performance Baselines

Expected baseline performance (PostgreSQL adapter, 10K rows):

| Operation | Latency | Throughput |
|-----------|---------|-----------|
| Parse | 0.5-1ms | N/A |
| Authorization | 0.1-0.5ms | N/A |
| Query Planning | 1-2ms | N/A |
| Database Execution | 5-50ms* | 20-200 ops/sec* |
| Result Projection | 0.5-2ms | N/A |
| **Total End-to-End** | **7-55ms** | **18-190 ops/sec** |

*Depends on dataset size and query complexity

### CI/CD Integration

#### GitHub Actions

Benchmarks run on each PR:

```yaml
- name: Run Benchmarks
  run: |
    cargo bench --features "postgres,wire-backend" -- --verbose

- name: Upload Results
  uses: actions/upload-artifact@v3
  with:
    name: benchmark-results
    path: target/criterion/
```

#### Local Baseline Comparison

To compare against a baseline branch:

```bash
# Save current baseline
git stash
git checkout main
cargo bench --bench adapter_comparison -- --save-baseline main

# Return to feature branch
git checkout -
git stash pop
cargo bench --bench adapter_comparison -- --baseline main
```

Criterion will show comparison with percentage differences.

## Troubleshooting

### Docker Services Won't Start

```bash
# Clean up any hanging containers
docker-compose down --volumes
docker system prune -f

# Try setup again
bash BENCHMARK_QUICK_START.sh setup
```

### PostgreSQL Connection Timeout

```bash
# Check if service is healthy
docker-compose ps

# View logs
docker-compose logs postgres

# Verify connection manually
psql postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql -c "SELECT 1;"
```

### Benchmark Results Too Noisy

Benchmarks may vary based on:
- System load
- Background processes
- Thermal throttling

Solutions:
- Run on idle system
- Run multiple times and average
- Increase sample size (Criterion default: 100)

```bash
cargo bench --bench adapter_comparison -- --sample-size 200
```

### Out of Memory During Large Benchmarks

1M row benchmarks require ~2-4 GB RAM:

```bash
# Run smaller dataset instead
bash BENCHMARK_QUICK_START.sh run-medium

# Or increase swap space
fallocate -l 4G /swapfile
chmod 600 /swapfile
mkswap /swapfile
swapon /swapfile
```

## Performance Optimization Tips

### 1. Use Criterion's Warm-up Phase

Criterion automatically warms up benchmarks (3s by default). Ensure background tasks complete before measurement starts.

### 2. Profile Hot Paths

Use `perf` or Flamegraph to identify bottlenecks:

```bash
cargo install flamegraph
cargo flamegraph --bench adapter_comparison -- --verbose
```

### 3. Cache Benchmark Data

Benchmarks load data once per run. To isolate specific operations:

```bash
// In benchmark code
let schema = setup_once();  // Call outside loop
group.bench_function("operation", |b| {
    b.iter(|| schema.execute_query(...))
});
```

### 4. Compare Against Previous Runs

Criterion stores baseline data in `target/criterion/`. Compare automatically:

```bash
cargo bench -- --verbose
# Compare against target/criterion/*/base/raw.json
```

## Further Reading

- **Criterion.rs Documentation**: https://bheisler.github.io/criterion.rs/book/
- **Performance Characteristics**: See `docs/architecture/performance/performance-characteristics.md`
- **Advanced Optimization**: See `docs/architecture/performance/advanced-optimization.md`

