# Phase 14: Pool & Cache Optimization

## Objective
Eliminate the 15% mutation throughput overhead caused by coarse-grained cache
invalidation, and unlock runtime pool resizing by migrating from `deadpool` to
`bb8`.

## Status
[ ] Not Started

## Background

### Cache invalidation overhead (documented in roadmap v2.2.0 known limitations)
When `cache_enabled = true`, mutations route through `CachedDatabaseAdapter`
which runs a synchronous full-key-scan invalidation after every write:
- Measured on fraiseql-v: 7,047 RPS → 6,019 RPS (~15% overhead)
- Root cause: `evict_all_for_view(view_name)` iterates all 64 shards and
  evicts every entry matching the view, regardless of which keys actually
  overlap with the mutated rows
- Fix: write-through targeted eviction — only evict cache keys whose
  `views` set intersects with the mutated entity

### Pool resizing (roadmap future item)
`deadpool-postgres` has no `resize()` API. `PoolPressureMonitor` emits
recommendations but cannot act on them. `bb8` supports `pool.resize()`.
Migration enables the monitor to actively tune pool size at runtime.

## Success Criteria
- [ ] Mutation throughput within 3% of no-cache baseline (down from 15% overhead)
- [ ] `PoolPressureMonitor` can actively resize pool when `bb8` feature enabled
- [ ] All existing cache tests pass — zero behavior change for cache hits/misses
- [ ] `cargo nextest run --workspace` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] Benchmark gate: `velocitybench` M1 regression < 3% vs no-cache

---

## TDD Cycles

### Cycle 1: Targeted cache invalidation
**Crate**: `fraiseql-core`  
**Files**: `src/cache/adapter/mutation.rs`, `src/cache/result.rs`

**RED**:
- `targeted_eviction_only_removes_matching_keys` — insert 3 cache entries for
  views A, B, C; mutate entity tied to view A; assert B and C entries survive
- `full_scan_eviction_benchmark_vs_targeted` — criterion bench showing targeted
  eviction is ≤ full-scan time (should be strictly faster)
- `mutation_throughput_with_cache_within_3pct_of_no_cache` — k6 / internal
  benchmark gate (can be an `#[ignore]` integration test)

**GREEN**:
- Change `CachedDatabaseAdapter::execute_mutation` to call a new
  `ResultCache::evict_by_views(entity_views: &[ViewName])` instead of
  `evict_all_for_view`
- `evict_by_views`: iterate only the shards that contain keys whose `views`
  set intersects `entity_views`; skip shards where no intersection is possible
  (use a per-shard bloom filter or simply iterate and filter)
- Thread `entity_views` (already carried in `MutationDescriptor.invalidates`)
  through to the eviction call

**REFACTOR**: The bloom filter approach adds complexity. Start with simple
iterate-and-filter; add bloom filter only if benchmarks show shard iteration
is still the bottleneck.

**CLEANUP**: Update `docs/benchmarks/cache-invalidation.md` with new results.

---

### Cycle 2: `bb8` pool migration (opt-in feature)
**Crate**: `fraiseql-db`, `fraiseql-server`

**RED**:
- `pool_resizes_up_under_pressure` — integration test: start with pool size 2,
  simulate 20 concurrent queries, assert pool grows to ≥ 10 after pressure
- `pool_resizes_down_after_idle` — after pressure subsides, assert pool shrinks
  back toward min_size
- `pool_pressure_monitor_emits_active_resize_event` — assert Prometheus counter
  `fraiseql_pool_resize_total` increments

**GREEN**:
- Add `bb8` Cargo feature to `fraiseql-db` (default: off, `deadpool` still default)
- `#[cfg(feature = "bb8")]`: implement `DatabaseAdapter` for `bb8::Pool<PostgresConnectionManager>`
- `PoolPressureMonitor`: add `active_resize: bool` field; when `bb8` feature
  enabled and `active_resize = true`, call `pool.resize(new_size)` instead of
  just logging a recommendation
- Update `fraiseql.toml` schema: `[fraiseql.pool] active_resize = true` (only
  effective when `bb8` feature compiled in)

**REFACTOR**: Keep the `deadpool` path fully intact — `bb8` is additive. The
two paths share the `DatabaseAdapter` trait, no changes to query execution.

**CLEANUP**: Update `docs/known-limitations.md` to remove the "cannot resize"
note when `bb8` feature is enabled. Clippy, fmt, doc.

---

## Dependencies
- Requires: Phase 11 complete (multi-tenant executor uses the same pool infra)
- Parallel with: Phase 13 (independent subsystem)
- Blocks: nothing, but v2.3.0 release quality gate requires cache overhead < 3%

## Version target
v2.3.0
