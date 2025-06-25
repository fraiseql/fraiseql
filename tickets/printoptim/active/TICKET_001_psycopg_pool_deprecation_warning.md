# TICKET_001: Fix psycopg_pool AsyncConnectionPool Deprecation Warning

**Status:** Active
**Priority:** Medium
**Assigned to:** [Unassigned]
**Created:** 2025-01-24
**Updated:** 2025-01-24

## Description
The FraiseQL library is using a deprecated pattern for initializing async connection pools with psycopg_pool. This generates deprecation warnings when using the library. The connection pool is being opened in the constructor, which is deprecated and will be removed in future versions of psycopg.

## Warning Details
```
DeprecationWarning: AsyncConnectionPool opened in the constructor is deprecated. 
Please use AsyncConnectionPool.open() or 'async with' instead.
```

## Current Implementation (Deprecated)
```python
# In fraiseql library code
pool = AsyncConnectionPool(...)  # Opens in constructor
```

## Required Implementation
```python
# Option 1: Explicit open
pool = AsyncConnectionPool(...)
await pool.open()

# Option 2: Context manager
async with AsyncConnectionPool(...) as pool:
    ...
```

## Acceptance Criteria
- [ ] Identify all places in FraiseQL where AsyncConnectionPool is instantiated
- [ ] Update code to use the new pattern (either explicit open or context manager)
- [ ] Ensure all async connection pools are properly opened before use
- [ ] Verify no deprecation warnings are shown
- [ ] Update any related documentation or examples
- [ ] Ensure backward compatibility is maintained

## Technical Details
- Library: psycopg_pool (part of psycopg3)
- Affected component: Database connection pooling
- Risk: Currently just a warning, but will break in future psycopg versions

## Notes
- This is an internal FraiseQL library issue, not application code
- Should be fixed before psycopg removes the deprecated functionality
- Consider adding tests to ensure pools are properly initialized