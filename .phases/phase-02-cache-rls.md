# Phase 2: Cache RLS Guard Audit

## Objective

Verify that the `has_rls: bool` field on `CachedDatabaseAdapter` is correctly initialized
from schema configuration, and that it correctly bypasses the cache when no
`SecurityContext` is present in RLS-enabled schemas.

## Success Criteria

- [ ] `has_rls` is set to `true` when schema's RLS mode is Warn or Strict
- [ ] `has_rls` is set to `false` when schema's RLS mode is Off or absent
- [ ] When `has_rls=true` and `security_context=None`, cache read AND write are skipped
- [ ] When `has_rls=false`, cache operates normally regardless of security_context
- [ ] Cache key includes tenant_id when RLS is enabled (audit key composition)
- [ ] Two requests with same query but different tenant_ids produce different cache entries

## TDD Cycles

### Cycle 1: Initialization audit

- **RED**: Trace where `CachedDatabaseAdapter` is constructed in production code paths
  (server startup, hot-reload). Write a test that constructs it from a schema with
  `rls_mode: Strict` and asserts `has_rls == true`.
- **GREEN**: Wire `has_rls` from the schema's tenancy/RLS configuration at construction.
- **REFACTOR**: If `has_rls` is just checking a field, consider computing it lazily from
  the schema reference instead of storing a copy.
- **CLEANUP**: Lint.

### Cycle 2: Cache bypass for unauthenticated + RLS

- **RED**: Unit test with mock adapter: `has_rls=true`, call `execute_where_query` with
  `security_context=None`, verify the underlying adapter is called (no cache hit) and
  result is NOT written to cache.
- **GREEN**: Verify the existing `if self.has_rls && security_context.is_none()` guard
  in `query.rs` covers both read and write paths.
- **REFACTOR**: Add metric counter for skipped-cache events if missing.
- **CLEANUP**: Lint.

### Cycle 3: Cache key tenant isolation

- **RED**: Test that two requests with identical query/variables but different tenant_ids
  in their SecurityContext produce different cache keys.
- **GREEN**: Audit `cache_key_for_query()` — verify tenant_id is included in the hash.
  If not, add it.
- **REFACTOR**: Document the cache key format.
- **CLEANUP**: Lint.

## Dependencies

- Phase 1 (compiled schema must load correctly for the RLS config to be available)

## Status
[x] Complete
