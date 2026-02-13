# Benchmarking Strategy for fraiseql-wire

## Overview

This document outlines the approach to benchmarking fraiseql-wire against tokio-postgres and integrating performance testing into CI/CD.

## Questions to Answer

### 1. Should We Have Benchmarks in the Repository?

**YES, with caveats:**

#### Pros

- **Regression Detection**: Catch performance regressions before release
- **Design Validation**: Verify architectural decisions are sound
- **Documentation**: Benchmarks show real-world performance characteristics
- **Confidence**: Data-backed claims about performance
- **Community Trust**: Transparent performance measurement

#### Cons

- **Maintenance Burden**: Benchmarks break when dependencies update
- **Hardware Variance**: Results vary significantly across machines
- **CI Cost**: Running comprehensive benchmarks slows down CI
- **False Positives**: Noise from system load causes flaky results
- **Complexity**: Benchmark code adds maintenance overhead

#### Recommendation

**YES, but strategically:**

- Include **unit-level micro-benchmarks** (always run)
- Include **integration benchmarks** (optional in CI, run locally)
- Avoid **end-to-end benchmarks** in CI (too expensive/flaky)

---

### 2. Should We Include tokio-postgres Comparison?

**YES, but separate:**

#### Pros

- **Market Positioning**: Shows where fraiseql-wire excels
- **Decision Support**: Helps users choose the right tool
- **Competitive Verification**: Validates performance claims
- **Educational**: Demonstrates design trade-offs

#### Cons

- **Maintenance**: tokio-postgres updates may affect benchmarks
- **Scope Creep**: Benchmarking two projects is 2x work
- **Fairness**: Hard to compare apples-to-apples (different goals)
- **Dependencies**: Adds tokio-postgres as dev-dependency
- **CI Cost**: More benchmarks = slower CI

#### Recommendation

**YES, but separate from main benchmarks:**

- Create optional `benches/vs-tokio-postgres/` directory
- Mark as `#[ignore]` or exclude from default CI
- Run locally before releases
- Document results but don't track in CI
- Make comparison explicit about scope differences

---

### 3. Should Performance Tests Be in CI/CD?

**CONDITIONAL:**

#### For Always-Run CI

**Only micro-benchmarks** that are:

- Fast (< 1 second each)
- Deterministic (not sensitive to system load)
- Focused on unit-level operations
- Don't require real database

Examples:

- Protocol encoding/decoding speed
- JSON parsing performance
- Chunking strategy efficiency
- Error handling overhead

#### For Optional/Nightly CI

**Integration benchmarks** that:

- Require running Postgres
- Take 1-5 minutes to complete
- Measure end-to-end performance
- May be sensitive to system load

Examples:

- Throughput (rows/sec) against real Postgres
- Memory usage under sustained load
- Connection setup/teardown time
- Streaming latency characteristics

#### For Manual/Pre-Release

**Comparison benchmarks** that:

- Compare with tokio-postgres
- Require specific hardware
- Need statistical analysis
- Are run before major releases

#### Recommendation

**Tiered approach:**

1. **Always run** (< 5 sec): Fast unit benchmarks
2. **Nightly run** (5-10 min): Integration benchmarks
3. **Manual run**: Detailed comparisons

---

## Proposed Benchmark Structure

### Directory Layout

```
benches/
├── lib.rs                          # Shared benchmark utilities
├── micro_benchmarks.rs             # Unit-level benchmarks (ALWAYS RUN)
│   ├── protocol_encoding
│   ├── json_parsing
│   ├── chunking
│   └── error_handling
├── integration_benchmarks.rs       # Integration tests (NIGHTLY)
│   ├── streaming_throughput
│   ├── memory_usage
│   ├── connection_setup
│   └── latency_characteristics
└── vs-tokio-postgres/              # Comparison (MANUAL)
    ├── lib.rs
    ├── streaming_throughput.rs
    ├── memory_profile.rs
    └── README.md
```

### Cargo.toml Configuration

```toml
[[bench]]
name = "micro_benchmarks"
harness = false

[[bench]]
name = "integration_benchmarks"
harness = false

# Separate comparison benchmarks
[[bench]]
name = "vs-tokio-postgres"
path = "benches/vs-tokio-postgres/main.rs"
harness = false

[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }
tokio-postgres = { version = "0.7", optional = true }
postgres = { version = "0.19", optional = true }

[dev-dependencies.benchmark-data]
version = "0.1"
optional = true
```

---

## Benchmark Implementations

### Type 1: Micro-Benchmarks (Fast, Deterministic)

These run in every CI/CD pipeline. < 1 second each.

**Example: Protocol Encoding**

```rust
// benches/micro_benchmarks.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use fraiseql_wire::protocol::encode::{encode_query, encode_terminate};

fn bench_protocol_encoding(c: &mut Criterion) {
    c.bench_function("encode_query_simple", |b| {
        b.iter(|| {
            let query = black_box("SELECT data FROM v_user WHERE id = 1");
            encode_query(query)
        })
    });

    c.bench_function("encode_terminate", |b| {
        b.iter(|| encode_terminate())
    });
}

criterion_group!(benches, bench_protocol_encoding);
criterion_main!(benches);
```

**Example: JSON Parsing**

```rust
fn bench_json_parsing(c: &mut Criterion) {
    let json_str = r#"{"id": 1, "name": "test", "nested": {"x": [1,2,3]}}"#;

    c.bench_function("parse_json_small", |b| {
        b.iter(|| {
            serde_json::from_str::<serde_json::Value>(black_box(json_str))
        })
    });

    // Larger JSON
    let large_json = generate_large_json(1000); // 1000 fields
    c.bench_function("parse_json_large", |b| {
        b.iter(|| {
            serde_json::from_str::<serde_json::Value>(black_box(&large_json))
        })
    });
}
```

**Example: Chunking Strategy**

```rust
fn bench_chunking(c: &mut Criterion) {
    c.bench_function("chunking_strategy_small_chunks", |b| {
        b.iter(|| {
            let mut strategy = ChunkingStrategy::new(10); // 10-item chunks
            for i in 0..1000 {
                strategy.add_row(black_box(i));
            }
        })
    });

    c.bench_function("chunking_strategy_large_chunks", |b| {
        b.iter(|| {
            let mut strategy = ChunkingStrategy::new(1000);
            for i in 0..10000 {
                strategy.add_row(black_box(i));
            }
        })
    });
}
```

---

### Type 2: Integration Benchmarks (With Postgres)

These run nightly or on-demand. 1-5 minutes total.

**Example: Streaming Throughput**

```rust
// benches/integration_benchmarks.rs
#[tokio::main]
async fn bench_streaming_throughput(c: &mut Criterion) -> Result<()> {
    // Setup: Create test database with known data
    let client = setup_test_db().await?;

    let mut group = c.benchmark_group("streaming_throughput");
    group.sample_size(10); // Fewer samples for slower benchmarks

    group.bench_function("throughput_1k_rows", |b| {
        b.to_async(tokio::runtime::Runtime::new()?).iter(|| async {
            let mut stream = client
                .query("test_1k")
                .execute()
                .await?;

            let mut count = 0;
            while let Some(_) = stream.next().await {
                count += 1;
            }
            assert_eq!(count, 1000);
        })
    });

    group.bench_function("throughput_100k_rows", |b| {
        b.to_async(tokio::runtime::Runtime::new()?).iter(|| async {
            let mut stream = client
                .query("test_100k")
                .execute()
                .await?;

            let mut count = 0;
            while let Some(_) = stream.next().await {
                count += 1;
            }
            assert_eq!(count, 100000);
        })
    });

    group.finish();
    Ok(())
}
```

**Example: Memory Usage**

```rust
#[tokio::main]
async fn bench_memory_usage(c: &mut Criterion) -> Result<()> {
    let client = setup_test_db().await?;

    let mut group = c.benchmark_group("memory_usage");

    group.bench_function("memory_chunk_size_10", |b| {
        b.to_async(tokio::runtime::Runtime::new()?).iter(|| async {
            let mut stream = client
                .query("test_100k")
                .chunk_size(10)  // Small chunks
                .execute()
                .await?;

            while let Some(_) = stream.next().await {}
        })
    });

    group.bench_function("memory_chunk_size_1000", |b| {
        b.to_async(tokio::runtime::Runtime::new()?).iter(|| async {
            let mut stream = client
                .query("test_100k")
                .chunk_size(1000)  // Large chunks
                .execute()
                .await?;

            while let Some(_) = stream.next().await {}
        })
    });

    group.finish();
    Ok(())
}
```

---

### Type 3: Comparison Benchmarks (Optional)

These compare with tokio-postgres. Run before releases.

**Example: Throughput Comparison**

```rust
// benches/vs-tokio-postgres/streaming_throughput.rs
use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};

#[tokio::main]
async fn compare_throughput(c: &mut Criterion) -> Result<()> {
    // Setup both drivers
    let fraiseql_client = fraiseql_wire::FraiseClient::connect(...).await?;
    let tokio_pg_client = tokio_postgres::Client::connect(...).await?;

    let mut group = c.benchmark_group("throughput_comparison");

    for row_count in [1000, 10000, 100000].iter() {
        group.bench_with_input(
            BenchmarkId::new("fraiseql", row_count),
            row_count,
            |b, &count| {
                b.to_async(tokio::runtime::Runtime::new()?)
                    .iter(|| fraiseql_throughput(&fraiseql_client, count))
            },
        );

        group.bench_with_input(
            BenchmarkId::new("tokio-postgres", row_count),
            row_count,
            |b, &count| {
                b.to_async(tokio::runtime::Runtime::new()?)
                    .iter(|| tokio_pg_throughput(&tokio_pg_client, count))
            },
        );
    }

    group.finish();
    Ok(())
}
```

---

## CI/CD Integration

### GitHub Actions Configuration

```yaml
# .github/workflows/benchmark.yml
name: Benchmarks

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]
  schedule:
    # Run nightly benchmarks every day at 2 AM
    - cron: '0 2 * * *'

jobs:
  micro-benchmarks:
    # Always run - fast
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2

      - name: Run micro-benchmarks
        run: cargo bench --bench micro_benchmarks

      - name: Upload benchmark results
        uses: actions/upload-artifact@v3
        with:
          name: micro-benchmarks
          path: target/criterion/

  integration-benchmarks:
    # Nightly only - slower, needs Postgres
    if: github.event_name == 'schedule' || contains(github.event.head_commit.message, '[benchmark]')
    runs-on: ubuntu-latest

    services:
      postgres:
        image: postgres:17
        env:
          POSTGRES_DB: fraiseql_test
          POSTGRES_PASSWORD: postgres
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
        ports:
          - 5432:5432

    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2

      - name: Setup test data
        run: |
          psql -h localhost -U postgres -d fraiseql_test \
            -f tests/bench-data.sql
        env:
          PGPASSWORD: postgres

      - name: Run integration benchmarks
        run: cargo bench --bench integration_benchmarks
        env:
          POSTGRES_HOST: localhost
          POSTGRES_USER: postgres
          POSTGRES_PASSWORD: postgres
          POSTGRES_DB: fraiseql_test

      - name: Upload benchmark results
        uses: actions/upload-artifact@v3
        with:
          name: integration-benchmarks
          path: target/criterion/

  benchmark-comparison:
    # Manual trigger or release
    if: github.event_name == 'workflow_dispatch' || contains(github.ref, 'v')
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2

      - name: Run comparison benchmarks
        run: cargo bench --bench vs-tokio-postgres

      - name: Generate report
        run: |
          cargo run --example benchmark-report \
            -- target/criterion fraiseql-vs-tokio.html

      - name: Upload report
        uses: actions/upload-artifact@v3
        with:
          name: benchmark-report
          path: fraiseql-vs-tokio.html
```

### Benchmark Data Setup

```sql
-- tests/bench-data.sql
-- Create test views with known data for benchmarking

CREATE TEMP VIEW v_test_1k AS
SELECT json_build_object(
  'id', i,
  'name', 'user_' || i::text,
  'email', 'user' || i::text || '@example.com',
  'active', (i % 2 = 0),
  'score', (i * 3.14)::numeric
) AS data
FROM generate_series(1, 1000) i;

CREATE TEMP VIEW v_test_100k AS
SELECT json_build_object(
  'id', i,
  'name', 'user_' || i::text,
  'email', 'user' || i::text || '@example.com',
  'active', (i % 2 = 0),
  'score', (i * 3.14)::numeric,
  'metadata', json_build_object(
    'created_at', now() - (i || ' days')::interval,
    'tags', json_build_array('tag1', 'tag2', 'tag3')
  )
) AS data
FROM generate_series(1, 100000) i;

CREATE TEMP VIEW v_test_large_json AS
SELECT json_build_object(
  'id', i,
  'nested', json_build_object(
    'level1', json_build_object(
      'level2', json_build_object(
        'level3', json_build_object(
          'data', repeat('x', 1000)
        )
      )
    )
  ),
  'array', json_build_array(1, 2, 3, 4, 5)
) AS data
FROM generate_series(1, 10000) i;
```

---

## Performance Regression Detection

### Strategy

1. **Store baseline** in repository

   ```
   benchmarks/baselines/
   ├── v0.1.0.json
   ├── v0.2.0.json
   └── current.json
   ```

2. **Compare on each commit**

   ```bash
   # Run benchmark
   cargo bench --bench micro_benchmarks -- --baseline current

   # Compare result
   criterion compares current against baselines/v0.1.0.json
   ```

3. **Alert on regression**
   - If any benchmark > 10% slower, fail CI
   - If any benchmark > 5% slower, warn in PR
   - Require explicit approval to merge

4. **Track over time**

   ```rust
   // Save results
   cp target/criterion/current.json benchmarks/baselines/v0.1.0.json

   // Track in git
   git add benchmarks/baselines/
   ```

---

## Recommendations Summary

### ✅ DO Include

1. **Micro-benchmarks** (always in CI)
   - Protocol encoding/decoding
   - JSON parsing
   - Chunking strategy
   - Error handling
   - Fast (< 1 second total)

2. **Integration benchmarks** (nightly/on-demand CI)
   - Streaming throughput
   - Memory usage patterns
   - Connection setup time
   - With Docker Postgres

3. **Comparison benchmarks** (manual/release)
   - vs. tokio-postgres
   - vs. sqlx
   - Separate benchmark suite
   - Run before major releases

### ❌ DON'T Include

1. **End-to-end benchmarks in main CI**
   - Too slow (> 10 minutes)
   - Too flaky (hardware variance)
   - Too expensive (CI costs)

2. **Benchmarks without infrastructure**
   - Don't benchmark without real Postgres
   - Don't measure abstractions that don't matter
   - Don't benchmark error cases extensively

3. **Comparison benchmarks in CI**
   - tokio-postgres updates break them
   - Different projects, different goals
   - Hard to make fair comparison
   - Too much maintenance burden

---

## Implementation Plan

### Week 1: Micro-benchmarks

- [ ] Set up Criterion framework
- [ ] Implement protocol encoding benchmarks
- [ ] Implement JSON parsing benchmarks
- [ ] Implement chunking benchmarks
- [ ] Integrate into CI (always-run)

### Week 2: Integration Benchmarks

- [ ] Create benchmark data SQL
- [ ] Implement throughput benchmarks
- [ ] Implement memory usage benchmarks
- [ ] Set up nightly CI job
- [ ] Create HTML reports

### Week 3: Comparison Benchmarks

- [ ] Create vs-tokio-postgres suite
- [ ] Document measurement methodology
- [ ] Create comparison report template
- [ ] Manual CI trigger setup

### Week 4: Documentation & Refinement

- [ ] Update BENCHMARKING.md with results
- [ ] Create performance guide
- [ ] Document how to run locally
- [ ] Set up baseline tracking

---

## Conclusion

**Recommended approach:**

| Benchmark Type | Location | CI Job | Frequency | Purpose |
|---|---|---|---|---|
| Micro | `benches/micro_*` | Always | Every commit | Regression detection |
| Integration | `benches/integration_*` | Nightly | Daily | Real-world validation |
| Comparison | `benches/vs-*` | Manual | Pre-release | Market positioning |

This balances:

- **Fast CI** (micro-benchmarks only, ~30 sec)
- **Comprehensive testing** (nightly integration, ~5 min)
- **Accurate comparisons** (manual comparison, no CI noise)
- **Maintainability** (clear separation of concerns)

Start with micro-benchmarks, add integration benchmarks in Phase 7, add comparisons before v1.0.0 release.
