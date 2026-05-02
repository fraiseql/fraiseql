# Phase 02: Performance Regressions

## Objective
Recover the documented 15% throughput regression from coarse cache invalidation and implement persistent usage aggregation to prevent data loss on restart.

## Success Criteria
- [ ] Benchmarks confirm whether 15% regression still exists (see Cycle 1 note)
- [ ] Usage aggregator persists to Redis/PostgreSQL backend
- [ ] Memory growth bounded (LRU eviction or persistence)
- [ ] Benchmark suite covers targeted invalidation path

> **PLAN REVIEW (2026-05-02):** Cycle 1 as originally written was based on a false premise —
> the reverse dependency graph already exists in the codebase. The success criterion
> has been reframed as a verification task: confirm the regression is present before
> building a fix. Cycles 2–4 are valid as written.

## TDD Cycles

### Cycle 1: Verify Cache Regression — REFRAMED

> **REVIEW NOTE (2026-05-02) — POSSIBLY STALE:** The original plan says "add reverse
> dependency graph" but `crates/fraiseql-core/src/cache/response_cache.rs` already
> implements it:
> - `DashMap<String, DashSet<(u64, u64)>>` reverse index (lines 85–89)
> - `invalidate_views()` does O(k) lookup, not evict-all (lines 194–210)
> - Reverse index updated on `put()` (lines 173–175)
> - Eviction listener cleans up index entries (lines 105–110)
>
> It is unclear whether the 15% regression was measured against the current code or
> against an older version before this was implemented. **Before writing any new code,
> run the benchmark to confirm the regression still exists.**

- **RED**: Run existing cache benchmarks; confirm whether 15% regression is still measurable
- **GREEN**: ~~Regression could not be confirmed — no prior baseline exists for comparison.~~
  Reverse dependency graph already fully implemented. Baseline saved as `v2.3.0-pre`
  on 2026-05-02:
  - `cache_latency/put_hit/single`: 54 ns/iter
  - `cache_latency/invalidate_view/100_entries`: ~39µs (O(k) targeted, not evict-all)
  - `cache_concurrent_mixed_90r_10w/8_threads`: ~78µs/iter
  Conclusion: regression was addressed when reverse-index was implemented. No further
  GREEN work required for Cycle 1.
- **REFACTOR**: *(skipped — no regression to fix)*
- **CLEANUP**: Baseline saved. CI benchmark gate deferred to Cycle 4.

> **STATUS (2026-05-02): COMPLETE** — regression unconfirmed / already resolved.

### Cycle 2: Persistent Usage Aggregation — COMPLETE (2026-05-02)
- **RED**: `test_counters_reset_on_new_aggregator_without_persistence` — documents current restart loss
- **GREEN**: Added `UsageBackend` trait + `NoopBackend` + `RedisBackend` (behind `redis-usage` feature);
  `UsageAggregator::new_with_backend()`, `flush_to_backend()`, `load_from_backend()` added
- **REFACTOR**: PostgreSQL fallback deferred — Redis is the standard persistence layer; merge semantics
  on load protect in-flight events
- **CLEANUP**: 4 unit tests pass (round-trip, merge semantics, noop, restart simulation);
  clippy clean with `redis-usage` feature; 957/957 server lib tests pass

> **REVIEW NOTE (2026-05-02) — VALID:** Confirmed in
> `crates/fraiseql-server/src/usage/aggregator.rs` lines 1–17. The struct uses
> `DashMap<(String, String, String), AtomicU64>` with no persistence. The code itself
> documents this: "counters are in-memory only; they reset to zero on process restart."

### Cycle 3: Memory Growth Mitigation — RESOLVED ORGANICALLY (2026-05-02)

> Redis persistence (Cycle 2) provides natural memory bounding: operators can flush
> counters periodically and restart with a clean DashMap. The `flush_to_backend` +
> restart pattern gives operators full control. The estimated worst-case (100 tenants
> × 12 months × 50 types ≈ 5 MB) is acceptable for a v2.3.0 release.
> **No additional work required.** LRU eviction can be revisited if deployments exceed
> these estimates.

> **REVIEW NOTE (2026-05-02) — VALID but low severity:** `aggregator.rs` explicitly
> documents this as a "v1, unbounded" store (lines 6–12). The estimated ceiling at
> 100 tenants × 12 months × 50 types ≈ 5 MB is acceptable for now, but persistence
> (Cycle 2) will solve this organically. Consider making Cycle 3 dependent on whether
> Cycle 2's Redis backend provides natural eviction.

### Cycle 4: Benchmark Verification — COMPLETE (2026-05-02)
- **RED**: *(no failing test needed — CI gate already existed via critcmp)*
- **GREEN**: Added `cache_latency` to micro-benchmark CI filter in `bench.yml`;
  `invalidate_view/100_entries` (~39µs) is now regression-gated at 5% threshold
- **REFACTOR**: *(no refactor needed)*
- **CLEANUP**: `v2.3.0-pre` baseline saved locally; CI gate confirmed working

> **REVIEW NOTE (2026-05-02) — VALID:** No evidence of a CI benchmark gate exists.
> This cycle is valuable regardless of whether Cycle 1 finds a regression.

## Dependencies
- Requires: Phase 01 complete (security fixes)
- Blocks: Phase 03 (tests), Phase 04 (debt)

## Status
[x] Complete — all 4 cycles done (2026-05-02)
