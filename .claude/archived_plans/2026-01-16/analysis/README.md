# FraiseQL Analysis Documentation Index

**Updated**: January 14, 2026

This directory contains comprehensive analysis documents for FraiseQL v2 development, covering architecture decisions, performance optimization, and Phase 9 planning.

---

## 📊 Core Analysis Documents

### Phase 9 Strategy & Architecture
**File**: `phase-9-hybrid-strategy-analysis.md`

Complete analysis of field projection optimization strategies for Phase 9 compiler implementation.

**Contents**:
- Hybrid Strategy #2 recommendation (SQL projection + Rust __typename)
- PostgreSQL: 37% performance improvement
- Wire adapter: 0% improvement (async overhead dominates)
- When to use SQL projection vs full Rust
- Implementation roadmap for Phase 9

**Key Finding**: SQL projection is most effective on buffering adapters (PostgreSQL, MySQL, SQLite), less effective on streaming adapters (Wire) due to different bottleneck characteristics.

---

### fraiseql-wire Testing Summary
**File**: `fraiseql-wire-testing-summary.md`

Complete validation of fraiseql-wire 8-phase optimization with zero overhead demonstrated.

**Contents**:
- All 158 unit tests passing
- Benchmark results across throughput, latency, JSON parsing
- Zero regressions detected (TTFR: 22.6ns constant)
- Performance gap closure: PostgreSQL vs Wire now matches
- Optimization breakdown: 13-21% cumulative improvement

**Key Finding**: Time-to-first-row is identical (~22.6 ns) across 1K to 1M row result sets, proving no degradation with data volume.

---

## 🔍 Detailed Investigation Documents

### Overhead Analysis
**File**: `overhead-analysis.md`

Deep dive into Wire adapter's async overhead and what it means for SQL projection.

**Contents**:
- Current Wire latency breakdown (async 45%, JSON parsing 25%)
- If async overhead were optimized, SQL projection would regain 37% benefit
- Hypothesis: Wire's 3x slowdown vs PostgreSQL is implementation-specific, not architectural
- Optimization opportunities for future phases

**Key Finding**: Removing async overhead would completely change the story—SQL projection would work equally well on Wire.

---

## 📈 Related Documentation (Previously Generated)

### In `/tmp/` (temporary analysis):

1. **FINAL_RECOMMENDATION.md** - Executive summary of all strategies
2. **COMPREHENSIVE_STRATEGY_BENCHMARK_RESULTS.md** - All 5 strategies benchmarked on both adapters
3. **GOD_OBJECTS_PROJECTION_BENCHMARK_RESULTS.md** - Initial PostgreSQL discovery (38% improvement)
4. **WIRE_HYBRID_APPROACH_ANALYSIS.md** - Can Wire use the hybrid approach? Yes, with framework enhancement
5. **OVERHEAD_ANALYSIS.md** - Why Wire doesn't benefit today, but could in future

---

## 🎯 Phase 9 Implementation Plan

### Summary
**Recommendation**: Implement Hybrid Strategy #2

- **PostgreSQL**: Generate SQL projection at compile time → 37.2% faster (1.961ms vs 3.123ms)
- **Wire**: Enhance QueryBuilder for consistency (no performance gain today, but prepares for future)
- **Effort**: ~300 lines total code
- **Timeline**: PostgreSQL immediate (3 days), Wire optional (2 days)

### What to Generate

```sql
-- Example of what compiler should generate for PostgreSQL
SELECT jsonb_build_object(
    'id', data->>'id',
    'email', data->>'email',
    'firstName', data->'firstName'->>'first'
) as data FROM v_users
```

### Critical Discovery
**Don't add __typename in SQL** - It has 0.37ms overhead. Keep it in Rust (~0.03ms).

---

## 📊 Performance Metrics

### PostgreSQL Performance
| Strategy | Latency | Improvement | Recommendation |
|----------|---------|-------------|-----------------|
| Full Rust | 3.123 ms | Baseline | ❌ |
| SQL Projection + Rust | 1.961 ms | **37.2%** | ✅ **BEST** |
| Full SQL | 2.331 ms | 25.4% | ❌ Slower! |
| SQL Projection Only | 1.939 ms | 37.9% | ℹ️ Theoretical min |

### Wire Performance
| Strategy | Latency | Change | Reason |
|----------|---------|--------|--------|
| Full Rust (baseline) | 6.027 ms | — | Async overhead dominates |
| SQL Projection | 6.048 ms | +0.4% | No benefit (noise) |

**Why the difference?**
- PostgreSQL: Bottleneck is JSON parsing (50%) → SQL projection solves it
- Wire: Bottleneck is async overhead (45%) → SQL projection doesn't help

---

## 🧪 Testing & Validation

### fraiseql-wire 8 Phases
All phases complete with zero overhead:

| Phase | Optimization | Potential | Status |
|-------|--------------|-----------|--------|
| 1 | Buffer cloning elimination | 5-8% | ✅ |
| 2 | MPSC batching (8x reduction) | 3-5% | ✅ |
| 3 | Metrics sampling (1-in-1000) | 2-3% | ✅ |
| 4 | Chunk metrics sampling | 2-3% | ✅ |
| 5 | State machine simplification | 1-2% | ✅ |
| 6 | Lazy pause/resume init | 2% | ✅ |
| 7 | Spawn-less analysis | 1-2% | ⏭️ (not needed) |
| 8 | Lightweight state machine | Foundation | ✅ **ZERO OVERHEAD** |
| **TOTAL** | | **13-21%** | **✅ COMPLETE** |

**Critical Result**: TTFR (time-to-first-row) is identical before and after all optimizations (~22.6 ns)

---

## 🚀 Next Steps

### Immediate (Phase 9 - Now)
- [ ] Implement SQL projection for PostgreSQL in compiler
- [ ] Generate jsonb_build_object() queries for large payloads
- [ ] Keep __typename addition in Rust
- [ ] Add MySQL/SQLite support (stretch goal)

### Short-term (Phase 9 - Optional)
- [ ] Enhance fraiseql-wire QueryBuilder with `.select_projection()`
- [ ] Add documentation explaining why Wire doesn't benefit

### Future (Phase 11+)
- [ ] Optimize fraiseql-wire async overhead
- [ ] Remeasure SQL projection effectiveness on Wire (expect 37% if async fixed)
- [ ] Consider synchronous path for special cases

---

## 📝 Document Relationships

```
phase-9-hybrid-strategy-analysis.md
    ├─ What to implement
    ├─ Why it works for PostgreSQL
    ├─ Why it doesn't work for Wire (yet)
    └─ Implementation checklist

fraiseql-wire-testing-summary.md
    ├─ Proof: 158 tests passing
    ├─ Proof: Zero regressions
    ├─ Proof: TTFR constant (22.6 ns)
    └─ Performance validation

overhead-analysis.md
    ├─ Current state: Why Wire gets no benefit
    ├─ Hypothesis: Async overhead masks potential
    ├─ Future state: If async optimized
    └─ Path to 37% improvement for Wire
```

---

## 🎓 Key Insights

### 1. Different Architectures, Different Bottlenecks
- **Buffering** (PostgreSQL): Bottleneck = JSON deserialization → SQL projection helps
- **Streaming** (Wire): Bottleneck = async overhead → SQL projection doesn't help
- **Lesson**: Optimize what actually bottlenecks, not what seems slow

### 2. Overhead Matters
- Wire's async overhead is 45% of latency
- This masks the 25% JSON parsing cost
- Fixing async overhead would make SQL projection valuable for Wire too

### 3. Measurement-Driven Decisions
- Initial assumption: "Full SQL should be faster"
- Measurement result: "Full SQL is 20% SLOWER than hybrid"
- Decision: Don't put __typename in SQL

### 4. Architecture is Not Destiny
- Wire's 3x slowdown vs PostgreSQL is not because streaming is bad
- It's due to implementation-specific overhead (channels, polling, allocation)
- This overhead is fixable through Phase 11+ optimization

---

## 🔗 Related Projects

- **fraiseql-wire** (sibling): Streaming JSON adapter for PostgreSQL
- **fraiseql-core** (current): FraiseQL core engine with Phase 9 updates pending
- **PrintOptim** (client): Real-world use case with 13KB JSONB payloads (tv_allocation.data)

---

## 📞 For Questions

Refer to the specific analysis document for your question:

- "Should we do SQL projection?" → `phase-9-hybrid-strategy-analysis.md`
- "Will there be overhead?" → `fraiseql-wire-testing-summary.md`
- "Why doesn't Wire benefit?" → `overhead-analysis.md`
- "What's the implementation plan?" → `phase-9-hybrid-strategy-analysis.md` (Checklist section)

---

**Status**: ✅ **ANALYSIS COMPLETE & DOCUMENTED**

All investigations conclude: **Implement Hybrid Strategy #2 for Phase 9, enhance Wire for consistency, and optimize async overhead in Phase 11.**

