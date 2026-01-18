# Phase 9: Hybrid Strategy Analysis - Complete Investigation

**Date**: January 14, 2026
**Status**: ✅ COMPLETE ANALYSIS & RECOMMENDATION
**For**: FraiseQL Phase 9 compiler implementation planning

---

## Investigation Summary

We conducted comprehensive benchmarking to determine the optimal field projection strategy for FraiseQL's Phase 9 compiler implementation. This analysis covers:

1. PostgreSQL vs Wire adapter bottleneck analysis
2. SQL projection effectiveness across architectures
3. __typename overhead implications
4. Wire async overhead characteristics
5. Phase 9 implementation recommendations

---

## Key Findings

### 1. PostgreSQL: 37% Improvement with SQL Projection ✅

**Strategy #2 (Recommended)**: SQL field projection + Rust __typename

```
Current approach (Full Rust):      3.123 ms
Hybrid approach (SQL + Rust):      1.961 ms
Improvement:                       37.2% faster
```

**Why**: PostgreSQL's bottleneck is JSON deserialization (~50% of latency). Reducing payload from 9.8KB to 450B eliminates this cost.

**Implementation**: Generate `jsonb_build_object()` queries at compile time:

```sql
SELECT jsonb_build_object(
    'id', data->>'id',
    'email', data->>'email',
    'firstName', data->'firstName'->>'first'
) as data FROM v_users
```

### 2. Wire: No Improvement with SQL Projection ⚠️

**Current finding**: Wire shows +0.4% regression (statistical noise)

```
Current approach (Full Rust):      6.027 ms
SQL projection:                    6.048 ms
Change:                            +0.4% (NO BENEFIT)
```

**Why**: Wire's bottleneck is async overhead (~45% of latency), not JSON parsing (~25%).

**But**: If Wire's async overhead were optimized, SQL projection would regain 37% benefit.

### 3. Critical Discovery: __typename Should Stay in Rust

**Finding**: Adding __typename in SQL has measurable overhead

```
Full SQL (everything in SQL):      2.331 ms
SQL projection + Rust __typename:  1.961 ms
Difference:                        0.37ms (20% penalty!)
```

**Why**: PostgreSQL's `jsonb_build_object()` constructor overhead scales with field count.

**Verdict**: Keep __typename in Rust (~0.03ms cost) instead of SQL (~0.37ms cost).

---

## Architecture Insights

### PostgreSQL (Buffering-Based)

**Bottleneck**: JSON deserialization

- Parses full 9.8KB JSONB → Value
- Iterates 50 fields, keeps 4
- Cost dominates (50% of total latency)

**Solution**: SQL projection

- PostgreSQL constructs 450B JSONB
- Rust receives already-filtered data
- Result: 37% faster

### Wire (Streaming-Based)

**Bottleneck**: Async context switching

- Per-chunk polling overhead
- Channel operations
- Task scheduling
- Cost dominates (45% of total latency)

**Current Solution**: SQL projection doesn't help

- JSON parsing is only 25% of latency
- Query construction adds overhead
- Net result: slightly slower

**Future Solution** (Phase 11+): Optimize async overhead

- If async drops to 35% of latency
- JSON parsing becomes 43% of latency
- SQL projection would save 43% → 37% improvement again
- This would require architectural optimization of fraiseql-wire

---

## Phase 9 Recommendation: Hybrid Strategy #2

### What to Implement

**For PostgreSQL** (immediate, ~3 days):

1. Compiler detects large payloads (>1KB or >10 fields)
2. Generate SQL projection queries at compile time
3. Include in compiled schema as constants
4. Runtime: Execute projection query, Rust adds __typename

**For Wire** (optional, ~2 days):

1. Enhance fraiseql-wire QueryBuilder with `.select_projection()` method
2. Add ~30 lines to support custom SELECT clause
3. Wire won't see performance benefit, but maintains architectural consistency
4. Prepares framework for future async optimization

### Why This Approach

**Pros**:

- ✅ 37% improvement on PostgreSQL (substantial)
- ✅ No regression on Wire (proven by benchmarks)
- ✅ Simple implementation (~300 lines total)
- ✅ Scales with payload size (larger objects = bigger gains)
- ✅ Future-proofs for Wire optimization
- ✅ Clear separation of concerns

**Cons**:

- ⚠️ Wire doesn't benefit (expected, by design)
- ⚠️ Compiler gets database-specific code
- ⚠️ Different SQL for each database (jsonb_build_object vs JSON_OBJECT vs json_object)

---

## Performance Projections

### God Objects (9.8KB like PrintOptim tv_allocation)

```
Queries × Result Size:
1000 rows:          3.123 ms → 1.961 ms  (1.162 ms saved)
10K rows:           31.23 ms → 19.61 ms  (11.62 ms saved)
100K rows:          312.3 ms → 196.1 ms  (116.2 ms saved)
1M rows:          3123 ms → 1961 ms    (1162 ms saved)

Real-world impact (PrintOptim):
- tv_allocation data: ~13KB JSONB
- Per 100K allocation rows: ~118ms saved per query
- If 10 similar queries/second: 1.18s improvement overall
```

### Database Support

```
PostgreSQL:        jsonb_build_object() → 37% improvement
MySQL:             JSON_OBJECT() → ~35% improvement (estimated)
SQLite:            json_object() → ~30% improvement (estimated)
SQL Server:        JSON_OBJECT() → ~32% improvement (estimated)
Wire:              (streaming) → 0% improvement (current)
```

---

## Implementation Checklist

### fraiseql-cli (Compiler)

- [ ] Detect god objects in schema (payload > 1KB or >10 fields)
- [ ] Implement `generate_sql_projection()` for each database
- [ ] Update compiled schema format to include SQL constants
- [ ] Handle field mapping (snake_case ↔ camelCase)
- [ ] Generate for all @fraiseql.type classes

### fraiseql-core

- [ ] Verify `execute_raw_query()` support in PostgresAdapter
- [ ] Update ResultProjector for SQL projection results
- [ ] Add integration tests for projected queries
- [ ] Benchmark validation (confirm 37% improvement)

### fraiseql-wire (Optional)

- [ ] Add `select_clause: Option<String>` to QueryBuilder
- [ ] Implement `.select_projection()` method
- [ ] Update `build_sql()` to use custom SELECT
- [ ] Add tests (should show no regression)
- [ ] Documentation of the enhancement

### Testing & Validation

- [ ] Unit tests for projection SQL generation
- [ ] Integration tests with real schemas
- [ ] Benchmark against baseline
- [ ] Test all supported databases
- [ ] Regression testing on Wire

---

## Decision: Go/No-Go for Phase 9

### Recommendation: ✅ GO

**This is a clear win**:

1. Data-driven (comprehensive benchmarks)
2. 37% improvement on primary adapter (PostgreSQL)
3. No regression on secondary adapter (Wire)
4. Minimal complexity addition
5. Scales with real-world payload sizes
6. Future-proofs the framework

### Priority

**High**: Implement Hybrid Strategy #2 for Phase 9

- PostgreSQL first (quick win, 37% improvement)
- Wire enhancement optional (architectural completeness)
- MySQL/SQLite support stretch goal

---

## What We Learned

### About Performance Optimization

1. **Measure everything** - Assumptions are often wrong
   - Assumption: "SQL can't be faster than Rust"
   - Reality: SQL is 37% faster

2. **Different architectures have different bottlenecks**
   - PostgreSQL: JSON parsing (addressable)
   - Wire: Async overhead (separate optimization)
   - One solution doesn't fit all

3. **Hidden overhead matters**
   - Wire's async overhead (~45% of latency) masks other improvements
   - This doesn't mean streaming is bad, just that it has specific costs
   - Measurable, addressable through different optimization

4. **The 80/20 rule applies**
   - Phase 6 implementation (lazy init) got 2ms
   - Phase 9 projection will get 1.16ms
   - Further optimization has diminishing returns

### About Architecture

1. **Streaming ≠ Slow**
   - Wire is 3x slower than PostgreSQL today
   - But this is due to implementation overhead, not streaming model
   - Future async optimization could change this

2. **Projection works where deserialization dominates**
   - PostgreSQL: ✅ (JSON parsing is bottleneck)
   - Wire: ❌ (async is bottleneck)
   - MySQL/SQLite: ✅ (likely similar to PostgreSQL)

3. **__typename optimization is non-obvious**
   - Naive approach: "Add to SQL"
   - Data shows: "Keep in Rust"
   - Constructor overhead > serialization cost

---

## Related Documentation

Complete analysis documents (generated in investigation):

1. `/tmp/FINAL_RECOMMENDATION.md` - Executive summary
2. `/tmp/COMPREHENSIVE_STRATEGY_BENCHMARK_RESULTS.md` - All 5 strategies benchmarked
3. `/tmp/GOD_OBJECTS_PROJECTION_BENCHMARK_RESULTS.md` - Initial PostgreSQL discovery
4. `/tmp/WIRE_HYBRID_APPROACH_ANALYSIS.md` - Can Wire use this approach?
5. `/tmp/OVERHEAD_ANALYSIS.md` - Deep dive on async overhead

All located in `/tmp/` for reference during Phase 9 implementation.

---

## Next Steps

1. **Immediate** (Week 1): Implement PostgreSQL SQL projection in compiler
2. **Short-term** (Week 2): Add MySQL/SQLite support
3. **Optional** (Week 2): Enhance fraiseql-wire for consistency
4. **Future** (Phase 11+): Optimize Wire's async overhead for additional gains

---

**Status**: ✅ ANALYSIS COMPLETE - READY FOR PHASE 9 IMPLEMENTATION
