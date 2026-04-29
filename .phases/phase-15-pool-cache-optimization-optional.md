# Phase 15: Pool & Cache Optimization (Optional / Post-v2.3.0)

## Objective
Explore whether `bb8` pool resizing and targeted cache invalidation yield
meaningful throughput gains on top of the already-world-class baseline.

## Status
[ ] Not Started — **low priority, do not start until v2.3.0 is shipped**

## Context

FraiseQL already benchmarks at **9,121 RPS on M1 mutations** (fraiseql-tv,
April 2026, `velocitybench`). This phase only makes sense if:

1. A new `velocitybench` run at higher concurrency (>40 workers) reveals a pool
   saturation bottleneck, OR
2. Profiling shows the cache eviction scan is measurably hot in production
   traces

**Do not start this phase speculatively.** Run the benchmark first.

## Go / No-Go Gate

Before any implementation:

```bash
# Run velocitybench at 2× worker count
cd ~/code/velocitybench
make bench WORKERS=80 DURATION=60s

# Profile cache eviction under mutation load
cargo flamegraph --bench mutation_throughput -- --bench
```

If M1 RPS does not improve ≥5% in profiling simulations → skip this phase
entirely. The 15% cache overhead is only ~1,300 RPS at the current baseline;
if the benchmark shows users saturating pool connections before that, pool
resizing matters more.

---

## Potential improvements (in priority order)

### A — Targeted cache eviction
**Crate**: `fraiseql-core`  
**Effort**: S (1–2 days)

Replace `evict_all_for_view(view_name)` with `evict_by_views(entity_views: &[ViewName])`
that only touches shards whose keys intersect the mutated entity. Expected
improvement: mutation RPS from ~9,100 → ~10,500 (eliminates ~15% overhead
when cache is enabled).

Only matters if `cache_enabled = true` is the common production config.
For the SpecQL free-tier model, cache may be disabled per-tenant — measure first.

### B — `bb8` pool migration (opt-in feature flag)
**Crate**: `fraiseql-db`  
**Effort**: M (3–5 days)

`deadpool-postgres` has no `pool.resize()` API. `bb8` supports runtime resizing,
enabling `PoolPressureMonitor` to act on its own recommendations instead of
just logging them.

Only matters if:
- Deployments run variable load (bursty traffic)
- Pool pressure metrics show frequent wait times > 50ms

For SpecQL's multi-tenant model (many idle tenants), this is more relevant than
for single-tenant deployments.

---

## Success Criteria (if pursued)

- [ ] `velocitybench` M1 regression gate: new result ≥ 5% improvement over baseline
- [ ] No regression on Q1/Q2b read benchmarks
- [ ] `bb8` path (if implemented) does not regress correctness: all integration tests pass
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean

## Dependencies
- Requires: Phase 14 (v2.3.0 finalized) — don't touch optimization before the
  feature set is stable
- Blocks: nothing

## Version target
v2.3.x patch or v2.4.0, decided based on benchmark results
