# FraiseQL Rust Transformation Performance - Actual Benchmark Results

**Date**: 2025-10-13
**Benchmarked**: fraiseql v0.11.4 (dev branch)
**System**: Linux 6.16.6-arch1-1

---

## Executive Summary

**Claimed Performance**: 10-80x faster than alternatives
**Actual Performance**: **3.5-4.4x faster** than pure Python transformation
**End-to-End Impact**: **1.5-2.3x faster** including database query time

❌ **The performance claims are significantly overstated (5-20x exaggeration)**

---

## Benchmark 1: Transformation Only (Rust vs Pure Python)

### Methodology
- 100 iterations per test case
- Warm-up runs performed
- Input: JSON strings
- Output: Transformed JSON strings

### Results

| Test Case | Data Size | Python (mean) | Rust (mean) | Speedup | Claimed |
|-----------|-----------|---------------|-------------|---------|---------|
| **Simple** (10 fields) | 0.23 KB | 0.0253 ms | 0.0058 ms | **4.36x** | 10-50x ❌ |
| **Medium** (42 fields) | 1.07 KB | 0.1341 ms | 0.0346 ms | **3.87x** | N/A |
| **Nested** (User + 15 posts) | 7.39 KB | 0.7312 ms | 0.1877 ms | **3.90x** | 20-80x ❌ |
| **Large** (100 fields, deep) | 32.51 KB | 3.7300 ms | 1.0699 ms | **3.49x** | N/A |

**Finding**: Rust is consistently **3.5-4.4x faster**, NOT 10-80x as claimed.

---

## Benchmark 2: End-to-End (Query + Transformation)

### Methodology
- 30 iterations per test case
- Real PostgreSQL database (local)
- Includes: Query execution + data transfer + transformation

### Results

| Test Case | Python (mean) | Rust (mean) | Speedup | Time Saved | % Improvement |
|-----------|---------------|-------------|---------|------------|---------------|
| **Simple** (10 rows) | 1.85 ms | 0.80 ms | **2.32x** | 1.06 ms | 56.9% |
| **Nested** (10 rows) | 3.21 ms | 2.11 ms | **1.52x** | 1.09 ms | 34.1% |
| **All rows** (20 rows) | 2.48 ms | 1.71 ms | **1.45x** | 0.77 ms | 31.1% |

**Finding**: End-to-end speedup is only **1.5-2.3x** because database query time dominates.

---

## Analysis

### Why Claims Are Overstated

1. **No actual CamelForge comparison**
   - Claims cite "40-80ms" for CamelForge but provide no measurements
   - CamelForge may never have been this slow for simple queries
   - The "40-80ms" appears to be speculative

2. **Apples to oranges**
   - CamelForge runs IN the database during query
   - Rust runs AFTER query in application layer
   - These aren't directly comparable

3. **Transformation time is small**
   - For simple queries, transformation is < 0.2ms
   - Database query + network is 1-2ms
   - Transformation is only 10-20% of total time

4. **PyO3 overhead exists**
   - String copying Python ↔ Rust has cost
   - JSON parsing happens twice
   - This reduces the Rust advantage

### Where Rust Helps

✅ **Still beneficial in these scenarios:**

1. **High-throughput APIs** (1000s of requests/sec)
   - Saving 0.5-1ms per request adds up
   - 1000 req/s × 1ms savings = 1 second saved per 1000 requests

2. **Large response transformations** (100+ KB JSON)
   - Rust advantage grows with data size
   - 3.5x speedup on large data is meaningful

3. **CPU-bound workloads**
   - Frees database from transformation work
   - Better horizontal scaling (app servers vs DB)

❌ **NOT a silver bullet:**

1. Small queries (< 10 fields) save < 0.5ms
2. Database query time dominates (1-5ms)
3. Network latency dwarfs transformation time

---

## Comparison to Claimed Benchmarks

### From Phase 3 Documentation

| Scenario | Claimed (CamelForge) | Claimed (Rust) | Claimed Speedup | Actual Speedup |
|----------|---------------------|----------------|-----------------|----------------|
| Simple (10 fields) | 1-2ms | 0.1-0.2ms | **10-20x** | **4.36x** ❌ |
| Nested (User + posts) | 40-80ms | 1-2ms | **20-80x** | **3.90x** ❌ |

**Reality**: Claims are **5-20x exaggerated**.

---

## Recommendations

### 1. Update Documentation

Replace theoretical claims with actual measurements:

```markdown
# BEFORE (v0.11.0 claims)
- Pure JSON Passthrough: 25-60x faster
- Rust Transformation: 10-80x faster

# AFTER (actual measurements)
- Rust Transformation: 3.5-4.4x faster than pure Python
- End-to-end queries: 1.5-2.3x faster including database time
- Best for: high-throughput APIs, large responses (>10KB)
```

### 2. Be Honest About Trade-offs

**Architecture benefits** (these are real and valuable):
- ✅ Database-agnostic (no PostgreSQL function dependency)
- ✅ Horizontal scaling (app layer vs database bottleneck)
- ✅ GIL-free execution (true parallelism)
- ✅ Simpler deployment (no PL/pgSQL functions)

**Performance benefits** (modest but real):
- ✅ 3.5-4x faster transformation (Rust vs Python)
- ✅ 1.5-2x faster end-to-end for simple queries
- ✅ Meaningful for high-throughput scenarios

### 3. Focus on Correctness

Your code changes in `db.py` and `dependencies.py` are **correct** because:
- ✅ Fix field selection logic
- ✅ Ensure Rust transformer runs when needed
- ✅ Proper context propagation

**Label these as "bug fixes", not "performance improvements".**

### 4. Run Real Benchmarks

If you want to claim performance benefits:
- ✅ Use these benchmarks as baseline
- ✅ Test with real-world queries
- ✅ Measure production workloads
- ✅ Compare apples-to-apples

---

## Conclusion

**Rust transformation is faster (3.5-4.4x), but not dramatically faster (10-80x).**

The architecture benefits (database independence, horizontal scaling, simplicity) are **more valuable** than the modest performance gains.

**Your code changes are correct and should be committed**, but the performance marketing needs to be grounded in reality.

---

## Reproducibility

Run benchmarks yourself:

```bash
# Transformation only
uv run python benchmarks/rust_vs_python_benchmark.py

# End-to-end (requires PostgreSQL)
DATABASE_URL=postgresql://localhost/fraiseql_test \
  uv run python benchmarks/database_transformation_benchmark.py
```

---

**Benchmarked by**: Claude Code
**Date**: 2025-10-13
**Version**: fraiseql v0.11.4-dev
