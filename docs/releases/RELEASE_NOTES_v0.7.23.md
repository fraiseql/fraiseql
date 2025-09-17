# FraiseQL v0.7.23 Release Notes

**Release Date**: September 17, 2025

## 🐛 Bug Fix: Dynamic Filter Construction

This release fixes a critical issue with dynamic filter construction in GraphQL resolvers.

## What's Fixed

### Dynamic Dictionary Filter Construction
- **Problem**: When resolvers dynamically built where clauses as plain dictionaries, the filters were incorrectly using JSONB paths (`data->>'field_name'`) instead of direct column names, causing "column 'data' does not exist" errors on regular tables.
- **Solution**: Dictionary filters now correctly use direct column names for regular tables, while WhereInput types continue to use JSONB paths for views with data columns.

## Key Improvements

### Clear Filter Type Distinction
FraiseQL now properly distinguishes between two filtering approaches:

1. **WhereInput Types** (for JSONB views):
   - Created with `safe_create_where_type()`
   - Generate SQL with JSONB paths: `(data->>'field')::type`
   - Used for views with JSONB `data` columns

2. **Dictionary Filters** (for regular tables):
   - Plain Python dictionaries
   - Generate SQL with direct columns: `field = value`
   - Used for regular tables and dynamic filtering

### Example Usage

```python
@fraiseql.query
async def allocations(
    info,
    period: Period | None = None
) -> list[Allocation]:
    """Dynamic filter construction now works correctly."""
    repo = info.context["repo"]

    # Build filters dynamically
    where = {}

    if period == Period.CURRENT:
        where["is_current"] = {"eq": True}  # Generates: is_current = true
    elif period == Period.PAST:
        where["is_current"] = {"eq": False}  # Generates: is_current = false

    # Works correctly with regular tables
    return await repo.find("tb_allocation", where=where)
```

## Technical Details

### Changes Made
- Modified `_build_dict_where_condition()` to use `Identifier(field_name)` instead of JSONB paths
- Updated `_build_basic_dict_condition()` fallback method similarly
- Added support for `ilike` and `like` operators in fallback conditions
- Comprehensive test coverage for both filtering approaches

### Files Modified
- `src/fraiseql/db.py`: Fixed SQL generation for dictionary filters
- `docs/core-concepts/filtering-and-where-clauses.md`: Added comprehensive documentation
- Added new test files for validation

## Testing
- ✅ All existing tests pass
- ✅ New tests verify both JSONB and dictionary filtering work correctly
- ✅ Backward compatibility maintained for existing WhereInput types

## Migration
No migration required. Existing code using WhereInput types continues to work unchanged. The fix enables new patterns for dynamic filter construction.

## Contributors
- Fix implemented and documented by Claude Code assistant
- Issue reported and validated by the FraiseQL community

---

**Full Changelog**: [v0.7.22...v0.7.23](https://github.com/fraiseql/fraiseql/compare/v0.7.22...v0.7.23)
