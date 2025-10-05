# Release Notes - FraiseQL v0.10.1

## 🐛 Bugfix: TurboRouter Dual-Hash APQ Lookup

### Release Date: 2025-10-05
### Type: Bugfix

## Summary

This release fixes a critical bug where TurboRouter failed to activate for Apollo Client APQ requests when using dual-hash registration, causing queries to fall back to normal execution mode (600ms instead of <20ms).

## 🚨 Problem

When queries were registered with dual-hash support for Apollo Client APQ compatibility, TurboRouter would fail to find them during query execution if the query text hashed to the `apollo_client_hash` instead of the server hash.

### Affected Scenario
- Query registered with `register_with_raw_hash()` and `apollo_client_hash` set
- Query text from APQ store hashes to Apollo Client hash (different formatting)
- `TurboRegistry.get(query_text)` only checked normalized and raw hashes
- **Never checked** the `_apollo_hash_to_primary` mapping
- Result: TurboRouter not activated, falling back to normal mode

### Before (Broken) ❌
```python
# Registration (works correctly)
turbo_query = TurboQuery(
    graphql_query=query,
    sql_template="SELECT turbo.fn_get_allocations()::json",
    param_mapping={},
    operation_name="GetAllocations",
    apollo_client_hash="ce8fae62da0e..."  # Apollo Client hash
)
registry.register_with_raw_hash(turbo_query, "bfbd52ba9279...")  # Server hash

# Stores:
#   _queries["bfbd52ba9279..."] = turbo_query
#   _apollo_hash_to_primary["ce8fae62da0e..."] = "bfbd52ba9279..."

# Execution (fails to find query!)
result = mode_selector.select_mode(query_text, variables, context)
# → _can_use_turbo(query_text)
#   → registry.get(query_text)
#     → hash_query(query_text) = "ce8fae62da0e..." (Apollo format from APQ)
#     → Checks: _queries["ce8fae62da0e..."] ❌ Not found (it's in mapping, not _queries)
#     → Returns None
# → Falls back to ExecutionMode.NORMAL ❌
# Response: { execution: { mode: "normal", time_ms: 609.08 } } ❌
```

### After (Fixed) ✅
```python
# Same registration
turbo_query = TurboQuery(
    graphql_query=query,
    sql_template="SELECT turbo.fn_get_allocations()::json",
    param_mapping={},
    operation_name="GetAllocations",
    apollo_client_hash="ce8fae62da0e..."
)
registry.register_with_raw_hash(turbo_query, "bfbd52ba9279...")

# Execution (now finds query via apollo mapping!)
result = mode_selector.select_mode(query_text, variables, context)
# → _can_use_turbo(query_text)
#   → registry.get(query_text)
#     → hash_query(query_text) = "ce8fae62da0e..." (Apollo format)
#     → Checks: _queries["ce8fae62da0e..."] ❌ Not found
#     → ✨ NEW: Checks _apollo_hash_to_primary["ce8fae62da0e..."] ✅ Found!
#     → Returns _queries["bfbd52ba9279..."] ✅
# → Returns ExecutionMode.TURBO ✅
# Response: { execution: { mode: "turbo", time_ms: 12.45 } } ✅
```

## Impact

### Who is Affected?
- **Production applications using Apollo Client with APQ** - Most common GraphQL client
- **Queries with dual-hash registration** - Where client and server formatting differs
- **TurboRouter users** - Performance degradation from 20ms to 600ms+

### Severity: High
- **Performance**: 30x-50x slowdown (turbo ~15ms → normal ~600ms)
- **Scope**: Any query where Apollo Client formatting differs from server formatting
- **Frequency**: Affected queries execute in normal mode on every request
- **Production Impact**: Users experiencing slow API responses despite turbo registration

## Technical Details

### Root Cause

The `TurboRegistry.get()` method had 3 lookup strategies:
1. ✅ Normalized hash - `hash_query(query_text)`
2. ✅ Raw hash - `hash_query_raw(query_text)`
3. ❌ **Missing**: Apollo hash mapping check

The `_apollo_hash_to_primary` mapping existed and worked for `get_by_hash()`, but was never checked during query text lookup in `get()`.

### Fix Applied

Enhanced `TurboRegistry.get()` to check the apollo hash mapping:

```python
def get(self, query: str) -> TurboQuery | None:
    """Get a registered TurboQuery by GraphQL query string.

    This method tries multiple hash strategies for maximum compatibility:
    1. Normalized hash (default FraiseQL behavior)
    2. Raw hash (for backward compatibility with external registrations)
    3. Apollo hash mapping (for dual-hash queries)  # ✨ NEW
    """
    # Try normalized hash first
    normalized_hash = self.hash_query(query)
    if normalized_hash in self._queries:
        self._queries.move_to_end(normalized_hash)
        return self._queries[normalized_hash]

    # Try raw hash
    raw_hash = self.hash_query_raw(query)
    if raw_hash in self._queries:
        self._queries.move_to_end(raw_hash)
        return self._queries[raw_hash]

    # ✨ NEW: Try apollo_client_hash mapping for dual-hash queries
    if normalized_hash in self._apollo_hash_to_primary:
        primary_hash = self._apollo_hash_to_primary[normalized_hash]
        if primary_hash in self._queries:
            self._queries.move_to_end(primary_hash)
            return self._queries[primary_hash]

    if raw_hash in self._apollo_hash_to_primary:
        primary_hash = self._apollo_hash_to_primary[raw_hash]
        if primary_hash in self._queries:
            self._queries.move_to_end(primary_hash)
            return self._queries[primary_hash]

    return None
```

**File Changed**: `src/fraiseql/fastapi/turbo.py:174-216`

### Why This Works

When a query is registered with dual-hash support:
- Primary hash stored in `_queries`
- Apollo hash mapping stored in `_apollo_hash_to_primary`

When query text from APQ hashes to Apollo hash:
1. Direct lookup in `_queries` fails (Apollo hash not a primary key)
2. **NEW**: Check if hash exists in `_apollo_hash_to_primary` mapping
3. If found, resolve to primary hash and return the `TurboQuery`
4. TurboRouter activates successfully

### Performance Impact
- **No performance penalty** - mapping lookup is O(1) dict operation
- **Maintains LRU behavior** - moves found queries to end of OrderedDict
- **Same performance as before** for non-dual-hash queries

## Testing

### New Test Added
```python
def test_get_by_query_text_with_dual_hash_apollo_format(
    self,
    sample_query_with_params,
    fraiseql_server_hash,
    apollo_client_hash,
):
    """Test that get() works when query text hashes to apollo_client_hash.

    Reproduces GetAllocations bug: when a query is registered with dual-hash
    support, and the query text from APQ hashes to the apollo_client_hash,
    get() should still find it via _apollo_hash_to_primary mapping.
    """
    # ... test implementation validates the fix
```

**Test File**: `tests/test_apollo_client_apq_dual_hash.py`

### Test Results
✅ All 7 Apollo dual-hash tests pass
✅ All 5 hash issue tests pass
✅ All 15 TurboRouter integration tests pass
✅ All 25 turbo-related tests pass
✅ 100% backward compatibility maintained

## Migration Guide

### No Action Required ✅
This is a pure bugfix with **zero breaking changes**:

1. **Automatic fix** - Existing dual-hash registrations now work correctly
2. **No code changes needed** - Applications automatically benefit from the fix
3. **No schema changes** - Database registrations unchanged
4. **No configuration changes** - Everything continues working as before

### Upgrade

```bash
pip install fraiseql==0.10.1
```

### Verification

After upgrading, verify TurboRouter activates for Apollo Client APQ requests:

```python
# Your existing code - no changes needed
# Just verify the execution mode in response metadata

response = await graphql_app.execute(
    query_hash="ce8fae62da0e...",  # Apollo Client APQ hash
    variables={...}
)

# Before v0.10.1: { execution: { mode: "normal", time_ms: 600+ } } ❌
# After v0.10.1:  { execution: { mode: "turbo", time_ms: <20 } } ✅
```

## Benefits Summary

✅ **TurboRouter activates correctly** for Apollo Client APQ requests
✅ **30x-50x performance improvement** (600ms → 15ms)
✅ **Dual-hash support fully functional** for all query text lookups
✅ **100% backward compatible** - no code changes required
✅ **Apollo Client compatibility** - most common production GraphQL client
✅ **Production ready** - eliminates performance regression for dual-hash queries

## Related Links

- Issue Analysis: `/tmp/fraiseql_turbo_apq_issue.md`
- Branch: `bugfix/turbo-apq-hash-context`
- Test Coverage: `tests/test_apollo_client_apq_dual_hash.py`

## Acknowledgments

Thank you to the team for the detailed root cause analysis that identified this dual-hash lookup gap in the TurboRegistry.

---

**Note:** If you're using Apollo Client with APQ and dual-hash registration, upgrading to v0.10.1 will restore full TurboRouter performance for all your queries.
