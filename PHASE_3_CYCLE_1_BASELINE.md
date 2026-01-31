# Phase 3, Cycle 1: Baseline Benchmarking - Execution Report

**Date**: 2026-01-31
**Phase**: Phase 3: Performance Optimization
**Cycle**: 1 - Baseline Benchmarking
**Status**: 🔴 RED Phase (Baseline Collection)

---

## Objective

Establish comprehensive performance baselines for all FraiseQL features to serve as the foundation for optimization work in Cycles 2-5.

---

## Benchmark Infrastructure

### Available Benchmark Suites

| Benchmark | Location | Focus | Status |
|-----------|----------|-------|--------|
| **adapter_comparison** | fraiseql-core/benches/ | PostgreSQL vs FraiseWire throughput/latency/memory | ✅ Ready |
| **sql_projection_benchmark** | fraiseql-core/benches/ | Field projection optimization impact | ✅ Ready |
| **full_pipeline_comparison** | fraiseql-core/benches/ | End-to-end query execution | ✅ Ready |
| **federation_bench** | fraiseql-core/benches/ | Federation/multi-service performance | ✅ Ready |
| **saga_performance_bench** | fraiseql-core/benches/ | Distributed transaction coordination | ✅ Ready |
| **wire benchmarks** | fraiseql-wire/benches/ | Streaming client micro-benchmarks | ✅ Ready |
| **server benchmarks** | fraiseql-server/benches/ | HTTP server performance | ✅ Ready |

### Framework

**Criterion.rs** - Statistical benchmarking with:
- Deterministic measurements
- Confidence intervals
- Regression detection
- HTML report generation
- P50/P95/P99 percentiles

### Test Data

- **Size**: 1M+ rows for realistic testing
- **Schema**: PostgreSQL views with JSON data plane
- **Fields**: id, name, email, status, score, tags, metadata
- **Setup**: SQL fixtures in `benches/fixtures/`
- **Database**: PostgreSQL 17 (Docker Compose)

---

## Performance Targets

### Query Execution
- Simple queries: **<5ms**
- Complex (10-table join): **<50ms p95**
- Aggregations: **<20ms**
- With caching: **<1ms**

### Subscriptions
- Event delivery latency: **<100ms p95**
- Throughput: **>1K events/sec**
- Memory per subscription: **<100KB**

### Connection Pooling
- Acquisition latency: **<1ms**
- Reuse rate: **>90%**
- Pool saturation recovery: **<100ms**

### Caching
- Hit rate: **>80%**
- Eviction time: **<10ms**
- Memory overhead: **<10% of data**

### Arrow Flight (Target)
- Throughput: **>100K rows/sec**
- Memory: **<1MB per 1M rows**
- vs JSON: **15-50x faster**

---

## Baseline Measurement Plan

### Step 1: Environment Setup (Phase 3, Day 1)
- [ ] Verify Criterion.rs is configured
- [ ] Set up Docker Compose database stack
- [ ] Load benchmark fixture data (1M rows)
- [ ] Configure DATABASE_URL environment variable
- [ ] Verify benchmarks compile

### Step 2: Micro-Benchmarks (Phase 3, Day 1)
- [ ] adapter_comparison - Small dataset (10K rows)
- [ ] sql_projection_benchmark - Projection impact
- [ ] full_pipeline_comparison - Query execution flow

### Step 3: Integration Benchmarks (Phase 3, Day 2)
- [ ] adapter_comparison - Large dataset (100K-1M rows)
- [ ] federation_bench - Multi-service scenarios
- [ ] saga_performance_bench - Transaction coordination

### Step 4: Load Tests (Phase 3, Day 2)
- [ ] concurrent_load_test - Concurrency patterns
- [ ] stress_tests - Extended conditions
- [ ] chaos_tests - Failure scenarios

### Step 5: Analysis & Documentation (Phase 3, Day 2)
- [ ] Collect all results
- [ ] Generate Criterion HTML reports
- [ ] Analyze baselines vs targets
- [ ] Identify optimization opportunities
- [ ] Document findings

---

## Expected Baseline Results

### Adapter Comparison (PostgreSQL vs FraiseWire)

**Latency** (single client, sequential queries):
```
10K rows:
  PostgreSQL:    32ms (p50), 35ms (p95)
  FraiseWire:    33ms (p50), 36ms (p95)

100K rows:
  PostgreSQL:    320ms (p50), 350ms (p95)
  FraiseWire:    330ms (p50), 360ms (p95)

1M rows:
  PostgreSQL:    4.2s (p50), 4.5s (p95)
  FraiseWire:    4.0s (p50), 4.3s (p95) [5% faster]
```

**Throughput** (rows per second):
```
PostgreSQL:    ~300K rows/sec
FraiseWire:    ~300K rows/sec
Arrow (target): ~300K rows/sec
```

**Memory Usage** (peak allocation):
```
10K rows:
  PostgreSQL:    260 KB
  FraiseWire:    1.3 KB [200x improvement]

100K rows:
  PostgreSQL:    26 MB
  FraiseWire:    1.3 KB [20,000x improvement]

1M rows:
  PostgreSQL:    260 MB
  FraiseWire:    1.3 KB [200,000x improvement]
```

### SQL Projection Impact

```
With projection:    75% less network payload
Without:            Full JSONB objects transferred
Impact:             20-30% latency reduction expected
```

### Query Execution Pipeline

```
Parse:             <1ms
Validate:          <1ms
Plan:              <1ms
Bind:              <1ms
Execute (DB):      5-20ms (depends on data)
Project:           <1ms
Total:             5-22ms (typical)
```

### Connection Pool Metrics

```
Acquisition:       <1ms (typical)
Reuse rate:        >90% (pool size: 10)
Idle timeout:      900s (default)
Max connections:   10 (default, tunable)
```

### Cache Effectiveness

```
Hit rate:          >80% (after warmup)
Hit latency:       0.1ms
Miss latency:      5-30ms
Invalidation:      ~10ms (view-based)
```

---

## Execution Instructions

### Quick Start (3 Commands)

```bash
# 1. Setup (one-time, 5 minutes)
cd /home/lionel/code/fraiseql
docker-compose -f docker-compose.test.yml up -d
cargo bench --no-run

# 2. Run baseline (small: 2-5 minutes)
cargo bench --bench adapter_comparison -- --sample-size 10

# 3. View results
open target/criterion/report/index.html
```

### Full Baseline Execution

```bash
# Setup Docker (once)
docker-compose -f docker-compose.test.yml up -d
sleep 10

# Wait for databases
echo "Waiting for databases..."
sleep 5

# Run all benchmarks with default sample size
cargo bench 2>&1 | tee benchmark_results.txt

# Save results
mkdir -p baseline_$(date +%Y%m%d_%H%M%S)
cp -r target/criterion baseline_$(date +%Y%m%d_%H%M%S)/

# Generate summary report
cargo bench 2>&1 | grep -E "test result:|time:" > baseline_summary.txt
```

---

## Benchmark Details

### 1. adapter_comparison.rs (41 KB)

**What it measures**:
- PostgreSQL (tokio-postgres) vs FraiseWire (streaming)
- Throughput (rows/sec)
- Latency (p50, p95, p99)
- Memory usage

**Data sizes**:
- Small: 10K rows
- Medium: 100K rows
- Large: 1M rows

**Run time**: 5-10 minutes (depending on sample size)

### 2. sql_projection_benchmark.rs (13 KB)

**What it measures**:
- Impact of field-level projection
- Network payload reduction
- Latency improvement
- Memory savings

**Scenarios**:
- With projection (select specific fields)
- Without projection (full JSONB)
- Various field counts (5, 10, 20 fields)

**Run time**: 2-3 minutes

### 3. full_pipeline_comparison.rs (12 KB)

**What it measures**:
- Complete GraphQL execution flow
- Parse → Plan → Bind → Execute → Project
- End-to-end latency
- Throughput

**Queries**:
- Simple (single table)
- Complex (10-table join)
- Aggregations
- With filtering

**Run time**: 3-5 minutes

### 4. federation_bench.rs (8 KB)

**What it measures**:
- Multi-service queries
- Cross-database joins
- Federation overhead
- Latency vs single-service

**Scenarios**:
- Local (single service)
- Federation (2 services)
- Deep federation (3 services)

**Run time**: 3-4 minutes

### 5. saga_performance_bench.rs (24 KB)

**What it measures**:
- Multi-step transaction coordination
- Compensation logic
- LIFO ordering
- Event delivery

**Scenarios**:
- Simple saga (2 steps)
- Complex saga (5 steps)
- Concurrent sagas
- Under load

**Run time**: 4-6 minutes

---

## Measurement Methodology

### Statistical Rigor

1. **Sample Size**: Criterion auto-adjusts (default: 100 samples)
2. **Confidence Interval**: 95% (standard)
3. **Outlier Detection**: Automatic IQR-based filtering
4. **Regression Detection**: Automatic (compares to previous runs)

### Reproducibility

1. **Control Variables**:
   - Same machine (your laptop)
   - Same database (Docker Compose)
   - Same test data (SQL fixtures)
   - Same Rust toolchain (locked in Cargo.lock)

2. **Measurement Best Practices**:
   - Cold start (fresh database)
   - Warm start (after 10 queries)
   - Peak load (concurrent queries)
   - Sustained (30+ minute runs)

### Data Collection

```rust
// Example: What Criterion captures
criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(100)              // 100 iterations
        .measurement_time(Duration::from_secs(10));
    targets = bench_adapter_comparison
);

// Result: P50, P95, P99 latencies + throughput
```

---

## Analysis Framework

### For Each Benchmark

1. **Status vs Target**
   - ✅ PASS: Below target
   - ⚠️ CLOSE: Within 10% of target
   - ❌ FAIL: Exceeds target

2. **Optimization Opportunity**
   - Score = (Current / Target) * 100
   - >100%: Needs work
   - 80-100%: Close, minor tweaks
   - <80%: Exceeds expectations

3. **Next Steps**
   - If PASS: Document and move on
   - If CLOSE: Profile to find bottlenecks
   - If FAIL: Plan optimization

---

## Cycle 1 Success Criteria

- [ ] All benchmarks run without errors
- [ ] Results documented with p50/p95/p99
- [ ] Compare to targets (identify gaps)
- [ ] Identify top 3 optimization opportunities
- [ ] Create detailed bottleneck analysis
- [ ] Plan Cycle 2 optimizations
- [ ] Commit baseline data

---

## Baseline Storage

### Directory Structure

```
fraiseql/
├── baseline_$(date)/
│   ├── criterion/              # HTML reports
│   ├── benchmark_results.txt   # Raw output
│   └── summary.json            # Parsed results
├── PHASE_3_CYCLE_1_BASELINE.md # This file
└── .phases/
    └── phase-03-performance.md # Updated with results
```

### Files to Save

1. `benchmark_results.txt` - Full output
2. `target/criterion/report/index.html` - Visual results
3. `baseline_summary.json` - Parsed metrics
4. `.phases/PHASE_3_CYCLE_1_RESULTS.md` - Analysis

---

## Next Steps After Baseline

### Cycle 2: Quick Wins
1. Connection pool configuration guide
2. Cache effectiveness verification
3. SQL projection defaults
4. Parameter allocation optimization

### Cycle 3: Deeper Optimization
1. Memory profiling
2. Hot path analysis
3. Batch query support
4. Subscription optimization

### Cycles 4-5
1. Arrow Flight completion
2. Performance monitoring
3. Prometheus/Grafana integration
4. Tuning documentation

---

## Troubleshooting

### Benchmark Won't Run

```bash
# Check if database is up
docker-compose -f docker-compose.test.yml ps

# Check if test data loaded
psql $DATABASE_URL -c "SELECT count(*) FROM \"v_user\";"

# Rebuild benchmarks
cargo bench --no-run --release
```

### Results Seem Slow

```bash
# Check if debug build (benchmarks compile with optimizations)
cargo bench --release

# Check system load
top -n 1

# Check database
docker logs $(docker-compose -f docker-compose.test.yml ps -q postgres)
```

### Memory Usage Unexpected

```bash
# Use system monitoring
ps aux | grep fraiseql

# Profile with heaptrack (if available)
heaptrack /usr/bin/cargo bench
```

---

## Related Documentation

- `.phases/phase-03-performance.md` - Phase 3 plan
- `.phases/PHASE_3_ANALYSIS.md` - Codebase analysis
- `crates/fraiseql-core/benches/README.md` - Benchmark details
- `BENCHMARK_SETUP.md` - Comprehensive setup guide

---

**Status**: 🔴 RED Phase - Ready to execute baseline measurements
**Next**: Begin benchmark execution (Step 1: Environment Setup)
