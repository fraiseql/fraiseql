# Release Notes - FraiseQL v0.9.5

## 🐛 Critical Fix: Nested Object Filtering on Hybrid Tables

### Release Date: 2025-09-28
### Type: Bug Fix & Performance Enhancement

## Summary

This release fixes a critical issue with nested object filtering on hybrid tables (tables with both SQL columns and JSONB data), dramatically improving both correctness and query performance.

## 🚨 Issue Fixed

When using nested object filters on hybrid tables that have both dedicated SQL columns (e.g., `machine_id`) and equivalent JSONB paths (e.g., `data->'machine'->>'id'`), FraiseQL was incorrectly generating JSONB traversal queries instead of using the indexed SQL columns.

### Before (Incorrect) ❌
```graphql
# GraphQL Query
query GetAllocations($machineId: ID!) {
    allocations(where: {machine: {id: {eq: $machineId}}}) {
        id
        machine { id name }
    }
}
```

Generated SQL:
```sql
-- Inefficient JSONB traversal with type mismatch
WHERE (data -> 'machine' ->> 'id') = '01513100-...'
-- ❌ Slow JSONB parsing
-- ❌ Type error: text = uuid
-- ❌ Cannot use indexes
```

### After (Correct) ✅
```sql
-- Direct indexed column access
WHERE machine_id = '01513100-...'
-- ✅ 10-100x faster
-- ✅ Type-safe UUID comparison
-- ✅ Uses indexes
```

## Impact

### Who is Affected?
- Applications using hybrid tables with both SQL columns and JSONB data
- GraphQL queries with nested object filters (e.g., `{parent: {field: {operator: value}}}`)
- Any system using FraiseQL's `register_type_for_view()` with `has_jsonb_data=True`

### Severity: Critical
- **Data Correctness**: Queries were returning incorrect results
- **Performance**: 10-100x slower than necessary
- **Errors**: "Unsupported operator: id" warnings in logs
- **Type Safety**: UUID/text comparison failures

## Technical Details

### Root Cause
The WHERE clause generator wasn't recognizing that nested object filters on hybrid tables should map to SQL foreign key columns instead of JSONB paths.

### Solution
1. **Detection**: Identify hybrid tables during WHERE clause processing
2. **Introspection**: Check available SQL columns vs JSONB paths
3. **Intelligent Routing**: Map `{machine: {id: ...}}` to `machine_id` column when available
4. **Fallback**: Use JSONB paths only when no SQL column exists

### Performance Improvements
- **10-100x faster** for indexed foreign key lookups
- **Type-safe** comparisons (UUID = UUID vs text = UUID)
- **Index-friendly** queries that PostgreSQL can optimize
- **Reduced CPU** from eliminating JSONB parsing overhead

## Migration Guide

### No Action Required ✅
This fix is completely transparent to your application:

1. **Automatic Optimization** - Existing queries will automatically use the faster SQL columns
2. **Backward Compatible** - All existing code continues to work
3. **No Schema Changes** - Your database structure remains unchanged

### Verification
To verify the improvement, check your PostgreSQL query logs:

```sql
-- Before v0.9.5: Slow JSONB query
EXPLAIN ANALYZE SELECT * FROM allocations
WHERE (data -> 'machine' ->> 'id') = '...';
-- Seq Scan, ~50ms

-- After v0.9.5: Fast indexed query
EXPLAIN ANALYZE SELECT * FROM allocations
WHERE machine_id = '...';
-- Index Scan, ~0.5ms (100x faster!)
```

## Additional Improvements

### WhereInput on Regular Tables
As a side effect of this fix, `WhereInput` types now work correctly on regular (non-JSONB) tables, expanding the flexibility of your GraphQL filters.

## Upgrading

```bash
pip install fraiseql==0.9.5
```

## Example: Hybrid Table Setup

```python
# This type of setup now works correctly with nested filters
@fraiseql.type(sql_source="tv_allocation")
class Allocation(BaseGQLType):
    id: uuid.UUID
    machine: Machine | None      # Nested object from JSONB
    location: Location | None    # Another nested object
    status: str

register_type_for_view(
    "tv_allocation",
    Allocation,
    table_columns={
        "id", "machine_id", "location_id",  # SQL columns
        "status", "data"                     # JSONB column
    },
    has_jsonb_data=True
)

# Now this filter works correctly and FAST:
where = AllocationWhereInput(
    machine=MachineWhereInput(
        id=UUIDFilter(eq=machine_id)  # Uses machine_id column!
    )
)
```

## Related Links

- Pull Request: [#72](https://github.com/fraiseql/fraiseql/pull/72)
- Previous Fix: [v0.9.4 - Nested JSONB paths](https://github.com/fraiseql/fraiseql/releases/tag/v0.9.4)
- Issue Report: Internal report (fraiseql_issue_nested_object_filtering_hybrid_tables.md)

## Acknowledgments

Thank you to the PrintOptim team for the detailed bug report that helped identify this performance-critical issue.

---

**Note:** If you use hybrid tables with nested object filtering, we strongly recommend upgrading to v0.9.5 immediately for significant performance improvements and correct query results.
