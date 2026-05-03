# Phase 09: Hot-Reload Cache Rebind (TODO #184)

## Objective

Fix the documented limitation in `AppState::reload_schema` where a hot-reload
swaps the schema but does not re-wrap the raw adapter in a new
`CachedDatabaseAdapter`. After a reload, per-view TTL overrides from the new
schema are silently ignored until a full server restart.

## Success Criteria

- [ ] After a schema reload, the new schema's per-view TTL overrides take effect
  immediately (no restart required)
- [ ] The cache is cleared on reload (preventing stale entries from the old schema)
- [ ] `// TODO(#184)` comment removed
- [ ] Integration test confirms TTL override takes effect post-reload

## Background

### Current behaviour (`app_state.rs:322–325`)

```rust
// TODO(#184): hot-reload does not re-wrap the adapter in a new CachedDatabaseAdapter.
// Per-view TTL overrides from the new schema will not be applied until a full restart.
let new_executor = Arc::new(Executor::new(schema, adapter.clone()));
```

`reload_adapter` stores the **raw** adapter (`Arc<A>`). When building the new
executor, it passes the raw adapter directly — bypassing the
`CachedDatabaseAdapter<A>` wrapper that was set up at server start.

### What `CachedDatabaseAdapter` does

`CachedDatabaseAdapter` is constructed with the schema's TTL config:

- Default TTL from `schema.cache_config.default_ttl_seconds`
- Per-view overrides from `schema.view_cache_ttls` map

When the schema changes, these TTL tables need to be reconstructed with the
new schema's config.

### Fix strategy

The `AppState` needs to store enough context to re-wrap the adapter on reload:

- Store the `CacheConfig` (or a factory closure) alongside `reload_adapter`
- On reload: construct a new `CachedDatabaseAdapter<A>` with the new schema's
  TTL tables, then pass that to the new `Executor`

**Important constraint:** the underlying connection pool (`Arc<A>`) must be
reused (not recreated) to avoid connection churn.

## TDD Cycles

### Cycle 1: Reproduce the Bug

- **RED**: Write test `test_hot_reload_applies_new_ttl_overrides`:
  - Build `AppState` with schema S1 (view `v_user` TTL = 60s)
  - Record a cache put for `v_user`
  - Hot-reload with schema S2 (view `v_user` TTL = 5s)
  - Confirm the new TTL is in effect (new puts use 5s; old entries are cleared)
- **GREEN**: *(test should fail — confirms the bug)*

### Cycle 2: Fix reload_schema to Re-wrap the Adapter

- **RED**: *(test from Cycle 1 still failing)*
- **GREEN**:
  - Add `cache_config: Option<Arc<CacheConfig>>` to `AppState` internal state
  - Store it when `with_cache_and_config` is called
  - In `reload_schema`: if `cache_config` is present, construct
    `CachedDatabaseAdapter::new(adapter.clone(), new_schema.ttl_config())`;
    otherwise use the raw adapter directly
  - Clear existing cache entries after the swap (already done for `#[cfg(feature = "arrow")]` — generalize it)
- **REFACTOR**: Extract a `fn build_executor_with_cache(schema, adapter, cache_cfg) -> Executor<A>`
  helper to eliminate duplication between initial startup and hot-reload paths
- **CLEANUP**: Remove `// TODO(#184)` comment; update `reload_schema` doc comment

### Cycle 3: Edge Cases

- **RED**: Write tests for:
  1. Reload with same TTL config → cache not invalidated (no churn)
  2. Reload without cache configured → no regression (raw adapter still works)
  3. Concurrent reload attempt while rebuild is in progress → second attempt rejected
- **GREEN**: Fix any failures
- **REFACTOR**: Ensure `reload_lock` covers the full re-wrap operation
- **CLEANUP**: All tests pass; no new clippy warnings

## Dependencies

- Requires: Phase 07 (clean workspace build)
- Blocks: Phase 10 (finalize)

## Status

[ ] Not Started
