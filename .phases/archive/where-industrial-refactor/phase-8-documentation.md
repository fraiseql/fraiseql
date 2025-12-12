# Phase 8: Documentation and Migration Guide [QA]

## Objective

Comprehensive documentation of the new WHERE architecture, API docs, migration guide, and architecture documentation.

## Context

The refactor is complete. Now we document:
- How the new system works
- How to use explicit FK metadata
- Migration guide for users
- Architecture for maintainers
- Performance characteristics

## Files to Create

- `docs/where-architecture.md` - Architecture documentation
- `docs/where-migration-guide.md` - Migration guide for users
- `docs/performance.md` - Performance characteristics
- `CHANGELOG.md` - Release notes

## Files to Modify

- `README.md` - Update examples with FK metadata
- `src/fraiseql/where_clause.py` - Add comprehensive docstrings
- `src/fraiseql/where_normalization.py` - Add comprehensive docstrings
- `src/fraiseql/db.py` - Update docstrings for `register_type_for_view()`

## Implementation Steps

### Step 1: Architecture Documentation

Create `docs/where-architecture.md`:

```markdown
# WHERE Clause Architecture

## Overview

FraiseQL uses a three-layer architecture for WHERE clause processing:

```
User Input (dict/WhereInput)
    ↓
Normalization Layer
    ↓
Canonical WhereClause
    ↓
SQL Generation
    ↓
PostgreSQL
```

## Components

### 1. WhereClause (Canonical Representation)

`WhereClause` is the single source of truth for WHERE clauses. All inputs normalize to this format before SQL generation.

**Location:** `src/fraiseql/where_clause.py`

**Key Classes:**
- `FieldCondition`: Single filter condition
- `WhereClause`: Complete WHERE clause with logical operators

**Benefits:**
- Type-safe
- Inspectable (readable `repr`)
- Testable (equality checks)
- Cacheable (hashable)
- Single SQL generation implementation

### 2. Normalization Layer

Converts dict and WhereInput to canonical WhereClause.

**Location:** `src/fraiseql/where_normalization.py`

**Functions:**
- `normalize_dict_where()`: Dict → WhereClause
- `normalize_whereinput()`: WhereInput → WhereClause
- `_is_nested_object_filter()`: FK vs JSONB detection

**FK Detection Logic:**
1. Check explicit `fk_relationships` metadata
2. Check pattern: `{"id": {"eq": value}}`
3. Verify FK column exists in `table_columns`
4. If yes → use FK column, else → use JSONB path

### 3. SQL Generation

Single implementation in `WhereClause.to_sql()`.

**Strategies:**
- **fk_column**: `machine_id = %s` (optimized)
- **jsonb_path**: `data->'device'->>'name' = %s`
- **sql_column**: `status = %s`

## Data Flow

### Example: Nested Filter

**Input (WhereInput):**
```python
AllocationWhereInput(
    machine=MachineWhereInput(
        id=UUIDFilter(eq=UUID("123"))
    )
)
```

**Step 1: Convert to dict**
```python
{
    "machine": {
        "id": {
            "eq": UUID("123")
        }
    }
}
```

**Step 2: Normalize to WhereClause**
```python
WhereClause(
    conditions=[
        FieldCondition(
            field_path=["machine", "id"],
            operator="eq",
            value=UUID("123"),
            lookup_strategy="fk_column",  # ← Detected!
            target_column="machine_id"
        )
    ]
)
```

**Step 3: Generate SQL**
```sql
machine_id = $1
```

**Parameters:** `[UUID("123")]`

## FK Relationship Detection

### Explicit Metadata (Recommended)

```python
register_type_for_view(
    "tv_allocation",
    Allocation,
    table_columns={"id", "machine_id", "status", "data"},
    has_jsonb_data=True,
    fk_relationships={"machine": "machine_id"}  # ← Explicit
)
```

### Convention-Based (Fallback)

If `fk_relationships` not specified:
- `machine` field + `machine_id` in columns → FK
- `machine` field + no `machine_id` → JSONB

### Mixed FK + JSONB

```python
{
    "machine": {
        "id": {"eq": "123"},      # → machine_id = '123' (FK)
        "name": {"contains": "P"} # → data->'machine'->>'name' LIKE '%P%' (JSONB)
    }
}
```

Combined with AND:
```sql
machine_id = $1 AND data->'machine'->>'name' LIKE $2
```

## Performance

| Operation | Typical Time | Cached Time |
|-----------|--------------|-------------|
| Dict normalization | 0.3ms | N/A |
| WhereInput normalization | 0.4ms | 0.03ms |
| SQL generation | 0.15ms | 0.01ms |
| **Total overhead** | **<0.5ms** | **<0.05ms** |

Compared to query execution (1-100ms), normalization overhead is negligible.

## Testing Strategy

Five levels of testing:

1. **Unit**: Test WhereClause, FieldCondition
2. **Integration**: Test normalization functions
3. **Equivalence**: Dict and WhereInput produce same results
4. **Code Path**: Verify FK optimization used
5. **Performance**: Benchmark normalization overhead

## Extension Points

### Adding New Operators

1. Add to operator constants in `where_clause.py`
2. Add SQL generation logic in `FieldCondition.to_sql()`
3. Add Filter class in `graphql_where_generator.py`
4. Add tests

### Adding New Lookup Strategies

1. Add strategy to `FieldCondition.lookup_strategy` type
2. Add detection logic in `_is_nested_object_filter()`
3. Add SQL generation in `FieldCondition.to_sql()`
4. Add tests

## Debugging

### Enable Debug Logging

```python
import logging
logging.getLogger("fraiseql").setLevel(logging.DEBUG)
```

### Inspect WhereClause

```python
where = {"machine": {"id": {"eq": "123"}}}
clause = repo._normalize_where(where, "tv_allocation", {...})
print(repr(clause))  # Human-readable
```

### Check SQL Generated

```python
sql, params = clause.to_sql()
print(sql.as_string(None))  # Raw SQL
print(params)  # Parameters
```

### Verify FK Detection

Check logs for:
- `"FK nested object filter"` → FK optimization used ✅
- `"JSONB nested filter"` → JSONB path used
- `"Unsupported operator"` → Bug! 🐛
```

### Step 2: Migration Guide

Create `docs/where-migration-guide.md`:

```markdown
# WHERE Clause Migration Guide (v1.9.0)

## Summary of Changes

FraiseQL v1.9.0 includes a major refactor of WHERE clause processing:

✅ **Fixed:** Nested filters work correctly with WhereInput objects
✅ **Improved:** Explicit FK metadata for better performance
✅ **Simplified:** 50% reduction in WHERE-related code
✅ **Faster:** Caching reduces overhead to <0.05ms

## Breaking Changes

**None.** The refactor is 100% backward compatible.

Existing code continues to work without changes.

## Recommended Migrations

### 1. Add Explicit FK Metadata (Optional but Recommended)

**Before:**
```python
register_type_for_view(
    "tv_allocation",
    Allocation,
    table_columns={"id", "machine_id", "status", "data"},
    has_jsonb_data=True,
)
```

**After:**
```python
register_type_for_view(
    "tv_allocation",
    Allocation,
    table_columns={"id", "machine_id", "status", "data"},
    has_jsonb_data=True,
    fk_relationships={"machine": "machine_id"},  # ← Explicit FK
)
```

**Benefits:**
- Validation at startup (catch errors early)
- Self-documenting (clear which fields use FK)
- Faster (no runtime detection)

### 2. Use WhereInput for Type Safety (Recommended)

**Before (dict):**
```python
@fraiseql.query
async def allocations(info, machine_id: str | None = None):
    where = {"machine": {"id": {"eq": machine_id}}} if machine_id else None
    return await db.find("tv_allocation", where=where)
```

**After (WhereInput):**
```python
AllocationWhereInput = create_graphql_where_input(Allocation)

@fraiseql.query
async def allocations(info, where: AllocationWhereInput | None = None):
    return await db.find("tv_allocation", where=where)
```

**Benefits:**
- Type safety
- IDE autocomplete
- GraphQL schema generation
- Validation

### 3. Remove Workarounds for Nested Filter Bug

If you had workarounds for the WhereInput bug, remove them:

**Before (workaround):**
```python
# Manual conversion to avoid WhereInput bug
if where and where.machine:
    where_dict = {
        "machine": {
            "id": {"eq": where.machine.id.eq}
        }
    }
    return await db.find("tv_allocation", where=where_dict)
```

**After (fixed):**
```python
# WhereInput works correctly now
return await db.find("tv_allocation", where=where)
```

## Deprecations

### _convert_dict_where_to_sql()

**Status:** Deprecated (will be removed in v2.0.0)

**Migration:** Use `_normalize_where()` instead

**Before:**
```python
sql = repo._convert_dict_where_to_sql(where_dict, view_name, columns)
```

**After:**
```python
clause = repo._normalize_where(where_dict, view_name, columns)
sql, params = clause.to_sql()
```

## New Features

### WhereClause Inspection

You can now inspect the normalized WHERE clause:

```python
where = {"machine": {"id": {"eq": "123"}}}
clause = repo._normalize_where(where, "tv_allocation", columns)

print(repr(clause))
# WhereClause(FieldCondition(machine.id eq '123' → FK:machine_id))

print(f"Conditions: {len(clause.conditions)}")
print(f"FK optimizations: {sum(1 for c in clause.conditions if c.lookup_strategy == 'fk_column')}")
```

### Explicit FK Metadata

Declare FK relationships explicitly:

```python
register_type_for_view(
    "tv_allocation",
    Allocation,
    fk_relationships={
        "machine": "machine_id",
        "location": "location_id",
        "organization": "org_id",  # Non-standard FK name
    }
)
```

## Testing Your Migration

### 1. Run Existing Tests

```bash
pytest tests/
```

All existing tests should pass without changes.

### 2. Check for Warnings

```bash
pytest tests/ -v -W error::DeprecationWarning
```

Fix any deprecation warnings.

### 3. Verify FK Optimization

Enable debug logging to verify FK optimization is working:

```python
import logging
logging.getLogger("fraiseql").setLevel(logging.DEBUG)

# Run your queries, check logs for:
# "FK nested object filter" ✅
# "Unsupported operator" ❌ (should not appear)
```

### 4. Performance Testing

The refactor should have no measurable performance impact:

```python
# Before/after comparison
import time

start = time.time()
result = await db.find("tv_allocation", where=where_input)
elapsed = time.time() - start

# Should be within ±5% of previous version
```

## Troubleshooting

### "FK column not detected"

**Symptoms:** Query works but uses JSONB path instead of FK column

**Solution:** Add explicit `fk_relationships`:

```python
register_type_for_view(
    ...,
    fk_relationships={"machine": "machine_id"}
)
```

### "Unsupported WHERE type" Error

**Symptoms:** `TypeError: Unsupported WHERE type`

**Cause:** Using custom WHERE object not supported by normalization

**Solution:** Convert to dict or WhereInput before passing to `find()`

### Performance Regression

**Symptoms:** Queries slower after upgrade

**Cause:** Unusual - normalization adds <0.5ms overhead

**Solution:**
1. Check database query performance (not FraiseQL)
2. Enable debug logging to verify FK optimization
3. Report issue with reproduction case

## Getting Help

- **GitHub Issues:** https://github.com/yourusername/fraiseql/issues
- **Docs:** https://fraiseql.readthedocs.io
- **Discord:** https://discord.gg/fraiseql

## Rollback Plan

If issues arise, you can stay on v1.8.x:

```bash
pip install fraiseql==1.8.0
```

No schema changes or data migrations are required.
```

### Step 3: Update CHANGELOG

Add to `CHANGELOG.md`:

```markdown
## [1.9.0] - 2025-XX-XX

### Added
- Explicit FK metadata in `register_type_for_view()` with `fk_relationships` parameter
- `WhereClause` canonical representation for internal WHERE processing
- Comprehensive WHERE clause normalization layer
- Performance caching for repeated queries
- Debug logging for FK optimization detection

### Fixed
- **Major:** Nested object filters now work correctly with WhereInput objects (#XXX)
- WhereInput nested filters (e.g., `machine: {id: {eq: "123"}}`) now use FK columns instead of JSONB paths
- No more "Unsupported operator: id" warnings for valid nested filters

### Changed
- **Internal:** Complete refactor of WHERE clause processing (backward compatible)
- **Internal:** Single code path for WHERE processing (dict and WhereInput converge to WhereClause)
- **Internal:** 50% reduction in WHERE-related code (800+ lines removed)

### Deprecated
- `_convert_dict_where_to_sql()` - Use `_normalize_where()` instead (will be removed in v2.0.0)

### Performance
- WHERE normalization overhead: <0.5ms (negligible)
- Caching reduces repeated queries to <0.05ms
- FK optimization improves query performance 10-100x vs JSONB path

### Migration
- No breaking changes - fully backward compatible
- Recommended: Add explicit `fk_relationships` to `register_type_for_view()`
- See [Migration Guide](docs/where-migration-guide.md) for details
```

### Step 4: Update README Examples

Add FK metadata examples to README:

```markdown
## Type Registration

Register types with explicit FK relationships:

```python
from fraiseql.db import register_type_for_view

register_type_for_view(
    "tv_allocation",
    Allocation,
    table_columns={"id", "machine_id", "location_id", "status", "data"},
    has_jsonb_data=True,
    fk_relationships={
        "machine": "machine_id",      # Explicit FK mapping
        "location": "location_id",    # Explicit FK mapping
    }
)
```

This enables FK optimization for nested filters:

```python
# This query uses the FK column (fast):
allocations(where: {machine: {id: {eq: "123"}}})
# SQL: WHERE machine_id = $1

# Mixed FK + JSONB works too:
allocations(where: {machine: {id: {eq: "123"}, name: {contains: "Printer"}}})
# SQL: WHERE machine_id = $1 AND data->'machine'->>'name' LIKE $2
```
```

## Verification Commands

```bash
# Generate API docs
uv run sphinx-build -b html docs/ docs/_build/  # if using Sphinx

# Check documentation links
uv run pytest --doctest-modules src/fraiseql/

# Verify examples in docs work
# Extract and run code examples from markdown

# Spell check
aspell check docs/*.md

# Check markdown formatting
uv run markdownlint docs/
```

## Acceptance Criteria

- [ ] Architecture documentation complete (`docs/where-architecture.md`)
- [ ] Migration guide complete (`docs/where-migration-guide.md`)
- [ ] CHANGELOG updated with release notes
- [ ] README updated with FK metadata examples
- [ ] All docstrings comprehensive and accurate
- [ ] API documentation generated (if using autodoc)
- [ ] Examples in docs are tested and work
- [ ] No spelling errors
- [ ] Markdown formatted correctly

## Deliverables

1. **For Users:**
   - Migration guide
   - Updated README
   - CHANGELOG

2. **For Maintainers:**
   - Architecture documentation
   - Code comments
   - Test documentation

3. **For Contributors:**
   - Extension guide
   - Debugging guide
   - Performance characteristics

## Notes

Good documentation is the difference between a refactor that gets adopted and one that gets feared.

Make it easy for users to:
- Understand what changed
- Know if they need to do anything
- Migrate if they want to use new features
- Debug if something goes wrong

## Final Phase Complete

After this phase, the WHERE industrial refactor is **100% complete**:

✅ Canonical representation
✅ Normalization layer
✅ Single SQL generation path
✅ Explicit FK metadata
✅ Code cleanup
✅ Performance optimization
✅ Comprehensive documentation

**Time to ship! 🚀**
