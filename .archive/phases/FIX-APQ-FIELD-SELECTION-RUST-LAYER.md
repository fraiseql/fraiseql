# APQ Field Selection Fix - Rust HTTP Layer Implementation

**Date**: January 4, 2026
**Branch**: feature/phase-16-rust-http-server
**Issue**: APQ returns full payload instead of respecting field selection
**Root Cause**: FastAPI router caches full responses, breaking field selection
**Solution**: Implement fix in Rust HTTP layer (canonical implementation)

---

## Problem Analysis

### Current Architecture
- **Rust HTTP Layer** (`fraiseql_rs/src/http/`) = Primary implementation (Axum)
- **Python FastAPI Layer** = Compatibility wrapper for existing users
- **Rust APQ Module** (`fraiseql_rs/src/apq/`) = Already correct (query-only, no response caching)

### The Bug
Located in `src/fraiseql/fastapi/routers.py` lines 1390-1398:
```python
# ❌ WRONG: Caches full responses
store_response_in_cache(apq_hash, response, ...)
apq_backend.store_cached_response(apq_hash, response_json, ...)
```

This breaks APQ because:
1. Client 1: Queries with fields (id, name) → Response cached
2. Client 2: Same APQ hash but wants (id, email) → Gets cached response with all original fields
3. Field selection is ignored; wrong data returned

### Why Rust Layer is Correct
`fraiseql_rs/src/apq/mod.rs` only implements:
- ✅ Query storage (by hash)
- ✅ Query retrieval (by hash)
- ❌ NO response caching

The `ApqStorage` trait has NO methods for response caching - only query operations.

---

## Implementation Strategy

### Phase 1: Add Response Caching Types to Rust (OPTIONAL)
If response caching is ever needed for performance, add to `fraiseql_rs/src/apq/`:
```rust
pub trait ApqResponseCache: Send + Sync {
    // Cache responses per (query_hash, variables_hash, user_id, selection_set_hash)
    async fn get_response(...) -> Result<Option<Response>, Error>;
    async fn set_response(...) -> Result<(), Error>;
}
```

**NOTE**: This is NOT the fix - it's just documenting proper architecture IF response caching is ever needed.

### Phase 2: Fix Python FastAPI Layer (CURRENT FIX)
Remove response caching from `src/fraiseql/fastapi/routers.py`:

**Location 1** (lines 1145-1153):
```python
# ❌ REMOVE: Check for cached response
cached_response = handle_apq_request_with_cache(request, apq_backend, config, context=context)
if cached_response:
    logger.debug(f"APQ cache hit: {sha256_hash[:8]}...")
    return cached_response
```

**Location 2** (lines 1369-1387):
```python
# ❌ REMOVE: Store response in cache
store_response_in_cache(apq_hash, response, apq_backend, config, context=context)
apq_backend.store_cached_response(apq_hash, response_json, context=context)
```

**Location 3** (lines 1108-1110 imports):
```python
# ❌ REMOVE: Unused imports
from fraiseql.middleware.apq_caching import (
    get_apq_backend,
    handle_apq_request_with_cache,  # ← Remove this
)
```

### Phase 3: Document Architecture (IMPORTANT)
Add clear comments in `src/fraiseql/fastapi/routers.py`:
```python
# NOTE: APQ response caching is intentionally NOT implemented.
#
# APQ should only cache query strings (persisted queries), not responses.
# Caching responses breaks field selection because the same persisted query
# with different field selections would return identical cached data.
#
# Correct behavior:
# 1. Store query by hash (in ApqStorage)
# 2. On hash-only request, retrieve query by hash
# 3. Execute query normally with client's field selection
# 4. Return only the requested fields
#
# See: fraiseql_rs/src/apq/mod.rs for canonical implementation (Rust)
```

### Phase 4: Update FastAPI Config
Ensure default setting is correct in `src/fraiseql/fastapi/config.py`:
```python
apq_cache_responses: bool = False  # ← Already correct
```

---

## Testing Strategy (TDD: RED-GREEN-REFACTOR)

### RED: Write Failing Tests
Create `tests/integration/test_apq_field_selection.py`:
- Test that response caching code is NOT called
- Test that different field selections return different results
- Test that query caching still works

### GREEN: Implement Fix
1. Remove response caching calls
2. Keep query caching working
3. All tests pass

### REFACTOR: Clean Up
1. Remove unused imports
2. Improve comments
3. No logic changes

---

## Files to Modify

| File | Changes | Reason |
|------|---------|--------|
| `src/fraiseql/fastapi/routers.py` | Remove response caching (3 locations) | Fix the bug |
| `src/fraiseql/fastapi/routers.py` | Remove unused imports | Clean up |
| `src/fraiseql/fastapi/routers.py` | Add architectural comments | Document fix |
| `tests/integration/test_apq_field_selection.py` | Create new test file | Verify fix works |

## Files to NOT Modify

| File | Reason |
|------|--------|
| `fraiseql_rs/src/apq/mod.rs` | Already correct (query-only) |
| `fraiseql_rs/src/apq/storage.rs` | Already correct (query-only interface) |
| `fraiseql_rs/src/http/axum_server.rs` | Verify it doesn't cache responses |
| `src/fraiseql/fastapi/config.py` | Already correct (disabled by default) |

---

## Verification Checklist

- [ ] Remove response caching calls from routers.py (3 locations)
- [ ] Remove unused imports
- [ ] Create comprehensive tests
- [ ] All tests pass (67+ existing APQ tests + 6 new tests)
- [ ] No regressions in other functionality
- [ ] Code compiles (Rust and Python)
- [ ] Comments explain architectural decision

---

## Commit Message Template

```
fix(apq): disable response caching in FastAPI layer to fix field selection

APQ was caching full responses in the FastAPI router, which broke field
selection because the same persisted query with different field selections
would return identical cached data.

ARCHITECTURE:
- Rust HTTP layer (Axum): Source of truth, already correct
- Python FastAPI layer: Compatibility wrapper, had response caching bug
- Rust APQ module: Already query-only (no response caching)

FIX:
Remove response caching from src/fraiseql/fastapi/routers.py:
- Remove handle_apq_request_with_cache() check (lines 1145-1153)
- Remove store_response_in_cache() call (lines 1369-1387)
- Remove unused imports

TESTING:
✅ 6 new APQ field selection tests
✅ 67+ existing APQ tests (no regressions)

RATIONALE:
APQ should only cache query strings, not responses. Each request must
execute the query to apply correct field selection and authorization.
```

---

## Architectural Notes

### Why Rust Layer is Canon
1. **Axum server** is the primary implementation (phase-16 goal)
2. **FastAPI layer** exists only for backward compatibility
3. **Rust APQ module** enforces query-only storage design
4. Bug fix must be in FastAPI layer to prevent divergence

### Future Improvements
1. Deprecate FastAPI-specific APQ handling
2. Move all APQ logic to Rust HTTP layer
3. Have FastAPI delegate to Rust for APQ operations
4. This ensures Axum and FastAPI use identical code paths

---

## Related Issues
- Apollo Client sends APQ hash, expects field selection respected
- Some clients cache responses locally; server must execute query each time
- Response caching only makes sense if query + variables + user_id + selection_set are cache key (impractical)

---

**Status**: READY FOR IMPLEMENTATION
**Target**: TDD (RED-GREEN-REFACTOR approach)
**Impact**: Bug fix only, no new features
**Risk**: Low (removing code, not adding)
