# FraiseQL v2 Baseline Performance Metrics

**Date**: 2026-01-13
**Purpose**: Establish baseline metrics before fraiseql-wire integration
**Database**: PostgreSQL (tokio-postgres driver)

---

## Executive Summary

This document establishes baseline performance metrics for FraiseQL v2's database query execution using the current tokio-postgres adapter. These metrics will be compared against fraiseql-wire performance to validate the 20,000x memory improvement claims.

---

## Methodology

### Test Environment

```
Database: PostgreSQL 17
Connection Pool: deadpool-postgres
Driver: tokio-postgres v0.7
Test Data: Generated JSON rows in v_test_data view
Measurements: criterion for latency, heaptrack for memory
```

### Query Patterns

All queries follow the FraiseQL pattern:

```sql
SELECT data
FROM v_{entity}
WHERE <predicate>
LIMIT <n>
```

### Test Sizes

- **Small**: 10,000 rows (~2.6 MB expected)
- **Medium**: 100,000 rows (~26 MB expected)
- **Large**: 1,000,000 rows (~260 MB expected)

---

## Baseline Metrics (tokio-postgres)

### Memory Usage

Expected baseline based on fraiseql-wire benchmarks:

| Query Size | Memory Usage (Heap) | Memory Usage (Total) | Notes |
|-----------|---------------------|----------------------|-------|
| 10K rows  | ~2.6 MB             | ~3.5 MB              | Entire result buffered in memory |
| 100K rows | ~26 MB              | ~30 MB               | **Target for comparison** |
| 1M rows   | ~260 MB             | ~280 MB              | Shows linear scaling with result size |

**Key Observation**: Memory usage is **O(result_size)** - proportional to total rows returned.

### Latency Metrics

Expected baseline:

| Metric | Value | Notes |
|--------|-------|-------|
| Connection Setup | ~250 ns | CPU-bound, cached connection pool |
| Query Parse | ~5-30 μs | SQL parsing overhead |
| Time-to-First-Row | ~2-5 ms | Network + query execution |
| Time-to-All-Rows (100K) | ~100-200 ms | Network I/O + deserialization |

**Key Observation**: Latency is primarily I/O-bound, not compute-bound.

### Throughput

Expected baseline:

| Query Size | Throughput | Notes |
|-----------|-----------|-------|
| 10K rows  | ~450K rows/sec | Network limited |
| 100K rows | ~480K rows/sec | Peak throughput |
| 1M rows   | ~420K rows/sec | Slight degradation due to buffering |

**Key Observation**: Throughput plateaus around 400-500K rows/sec, limited by network and deserialization.

---

## Actual Measurements (To Be Completed)

### Phase 0 Status

⚠️ **Benchmarks Not Yet Run**: The baseline benchmark suite has been created but not yet executed with actual database queries. This requires:

1. PostgreSQL test database setup with realistic data
2. Implementation of PostgresAdapter in benchmarks
3. Running benchmarks with memory profiling
4. Documenting actual measurements here

### Next Steps

1. **Setup Test Database**:

   ```bash
   # Create test database
   createdb fraiseql_bench

   # Generate test data (10K, 100K, 1M rows)
   psql fraiseql_bench -f tests/fixtures/generate_test_data.sql
   ```

2. **Run Benchmarks**:

   ```bash
   # Standard benchmarks
   DATABASE_URL=postgres://localhost/fraiseql_bench cargo bench --bench database_baseline

   # With memory profiling
   cargo build --release --benches
   heaptrack target/release/deps/database_baseline-*
   heaptrack_gui heaptrack.database_baseline.*.gz
   ```

3. **Document Results**:
   - Record actual memory usage from heaptrack
   - Record latency from criterion output
   - Calculate throughput (rows/sec)
   - Update this document with real measurements

---

## Comparison Framework

Once actual baseline is established, we'll compare:

### Memory Efficiency

```
Memory Reduction = (Baseline Memory - Wire Memory) / Baseline Memory × 100%

Expected for 100K rows:
  Baseline: 26 MB
  Wire:     1.3 KB
  Reduction: 99.995% (~20,000x)
```

### Latency Impact

```
Latency Delta = Wire Latency - Baseline Latency

Expected for time-to-first-row:
  Baseline: 2-5 ms
  Wire:     2-5 ms
  Delta:    ~0 ms (no regression expected)
```

### Throughput Maintenance

```
Throughput Ratio = Wire Throughput / Baseline Throughput

Expected for 100K rows:
  Baseline: 480K rows/sec
  Wire:     450-500K rows/sec
  Ratio:    0.94-1.04 (within 5% variance)
```

---

## Expected Advantages of fraiseql-wire

### Memory Advantage

**Before** (tokio-postgres):

- Entire result set buffered in memory
- 100K rows = 26 MB heap allocation
- Memory pressure scales linearly with query size

**After** (fraiseql-wire):

- Streaming with bounded chunks
- 100K rows = 1.3 KB heap allocation (chunk size)
- Memory constant regardless of query size

**Why It Matters**:

- Large list queries (e.g., `users(limit: 100000)`)
- Export operations (CSV, JSON streaming)
- Cursor-based pagination
- Real-time subscriptions (future feature)

### Performance Characteristics

| Metric | tokio-postgres | fraiseql-wire | Advantage |
|--------|----------------|---------------|-----------|
| **Memory (100K rows)** | 26 MB | 1.3 KB | **20,000x** |
| **Time-to-First-Row** | 2-5 ms | 2-5 ms | **No change** |
| **Throughput** | 480K rows/s | 450-500K rows/s | **Comparable** |
| **Connection Overhead** | 250 ns | 250 ns | **No change** |
| **Backpressure** | ❌ Buffered | ✅ Streaming | **Better** |

---

## Test Data Generation

To populate the test database:

```sql
-- Create test view with JSONB data
CREATE OR REPLACE VIEW v_test_data AS
SELECT
    row_number() OVER () AS id,
    jsonb_build_object(
        'id', gen_random_uuid(),
        'name', 'Test User ' || generate_series,
        'email', 'user' || generate_series || '@example.com',
        'status', CASE WHEN generate_series % 3 = 0 THEN 'active' ELSE 'inactive' END,
        'created_at', NOW() - (generate_series || ' days')::interval,
        'metadata', jsonb_build_object(
            'tags', ARRAY['tag1', 'tag2'],
            'score', (random() * 100)::int,
            'nested', jsonb_build_object('foo', 'bar', 'baz', generate_series)
        )
    ) AS data
FROM generate_series(1, 1000000);

-- Add index for performance
CREATE INDEX idx_test_data_status ON v_test_data USING gin ((data->'status'));
```

---

## Benchmark Execution Checklist

### Phase 0: Foundation

- [x] Build errors fixed (fact_tables, calendar_dimensions)
- [x] All tests compile and pass
- [x] Baseline benchmark suite created
- [ ] Test database setup with realistic data
- [ ] Benchmarks implemented with PostgresAdapter
- [ ] Memory profiling tools installed (heaptrack)

### Phase 0: Baseline Measurements

- [ ] Run benchmarks: 10K rows
- [ ] Run benchmarks: 100K rows
- [ ] Run benchmarks: 1M rows
- [ ] Memory profiling with heaptrack
- [ ] Document actual measurements
- [ ] Validate against expected values

### Phase 1: fraiseql-wire Comparison

- [ ] Implement FraiseWireAdapter
- [ ] Run same benchmarks with wire backend
- [ ] Compare memory usage (target: 20,000x reduction)
- [ ] Compare latency (target: <5% variance)
- [ ] Compare throughput (target: <5% variance)
- [ ] Document findings

---

## Success Criteria

### Memory Reduction

✅ **Target**: 1000x+ memory reduction for 100K row queries

- Baseline: 26 MB
- Wire: <26 KB (< 0.1% of baseline)

### No Latency Regression

✅ **Target**: <5% latency variance

- Time-to-first-row: ±0.5 ms
- Total query time: ±10 ms for 100K rows

### Throughput Maintenance

✅ **Target**: >95% throughput maintenance

- Baseline: 480K rows/sec
- Wire: >450K rows/sec

---

## Known Limitations

### Current Baseline Benchmark

**Limitations**:

1. **No actual database queries yet**: Benchmarks use placeholder sleep() calls
2. **No PostgresAdapter integration**: Requires implementing adapter in benchmark code
3. **No test data**: Requires setting up test database with 1M+ rows
4. **No memory profiling**: Requires heaptrack or similar tool installation

**Next Actions**:

1. Implement PostgresAdapter usage in benchmark
2. Generate test data in PostgreSQL
3. Run benchmarks with DATABASE_URL environment variable
4. Profile memory usage with heaptrack
5. Update this document with real measurements

---

## References

- **fraiseql-wire benchmarks**: `/home/lionel/code/fraiseql-wire/benches/COMPARISON_GUIDE.md`
- **fraiseql-wire performance**: `/home/lionel/code/fraiseql-wire/PERFORMANCE_TUNING.md`
- **Integration assessment**: `.claude/analysis/fraiseql-wire-integration-assessment.md`
- **Implementation plan**: `.claude/plans/fraiseql-wire-integration-plan.md`

---

**Status**: ⏳ **BASELINE SUITE CREATED, MEASUREMENTS PENDING**

The benchmark infrastructure is ready. Next step: Implement PostgresAdapter in benchmarks and run actual measurements.
