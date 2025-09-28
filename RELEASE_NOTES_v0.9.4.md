# Release Notes - FraiseQL v0.9.4

## 🐛 Critical Bug Fix: Nested Object Filtering in JSONB

### Release Date: 2025-09-28
### Type: Bug Fix Release

## Summary

This release fixes a critical bug in nested object filtering for JSONB-backed tables where GraphQL WHERE clauses were generating incorrect SQL, causing filters to fail silently and return unfiltered results.

## 🚨 Issue Fixed

When using nested object filters in GraphQL WHERE clauses with JSONB-backed tables, FraiseQL was generating incorrect SQL that accessed fields at the root level instead of the proper nested path.

### Before (Incorrect) ❌
```sql
-- Filter: {machine: {id: {eq: "01513100-..."}}}
WHERE (data ->> 'id') = '01513100-0000-0000-0000-000000000066'
-- This incorrectly looks for 'id' at the root of the JSONB data
```

### After (Correct) ✅
```sql
-- Filter: {machine: {id: {eq: "01513100-..."}}}
WHERE (data -> 'machine' ->> 'id') = '01513100-0000-0000-0000-000000000066'
-- This correctly navigates to machine.id in the nested JSONB structure
```

## Impact

### Who is Affected?
- Applications using JSONB-backed tables with nested object structures
- GraphQL queries filtering on nested object fields
- Any WHERE clause involving nested relationships in JSONB data

### Severity: High
- Filters were silently failing, returning unfiltered results
- Could lead to data exposure or incorrect query results
- No error messages were generated, making the issue hard to detect

## Technical Details

### Root Cause
The `to_sql()` method in the WHERE clause generator wasn't passing the parent path context when processing nested objects, causing all field access to default to the root level.

### Solution
- Modified `where_generator.py` to pass `parent_path` parameter through the entire `to_sql()` method chain
- Added `_build_nested_path()` helper function for clean path construction
- Updated the `DynamicType` Protocol to support the new parameter
- Fixed logical operators (AND, OR, NOT) to maintain parent path context

### Testing
- Enhanced existing tests to validate correct JSONB path generation
- Added deep nesting test coverage (3+ levels)
- All 3283 tests pass with no regressions

## Migration Guide

### No Action Required ✅
This is a bug fix that corrects incorrect behavior. Your existing code will automatically benefit from the fix:

1. **Existing filters will now work correctly** - Nested object filters that were silently failing will now properly filter results
2. **No code changes needed** - The fix is transparent to the API
3. **Backward compatible** - All existing queries continue to work

### Verification Steps
To verify your nested filters are working correctly after upgrading:

```python
# Example: Verify nested filtering works
allocations = await repo.find(
    where={
        "machine": {
            "id": {"eq": machine_id}  # This now correctly filters
        }
    }
)
```

## Upgrading

```bash
pip install fraiseql==0.9.4
```

## Related Links

- Pull Request: [#71](https://github.com/fraiseql/fraiseql/pull/71)
- Issue Report: Internal bug report (fraiseql_nested_filter_bug_report.md)
- Test Coverage: See `tests/integration/database/repository/test_nested_object_filter_integration.py`

## Acknowledgments

Thank you to the PrintOptim team for the detailed bug report that helped identify and fix this critical issue.

---

**Note:** If you rely on nested object filtering in JSONB tables, we strongly recommend upgrading to v0.9.4 immediately to ensure your filters work correctly.
