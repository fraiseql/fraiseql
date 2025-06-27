# Response: JSONB View Query Support in FraiseQL

## Summary

After analyzing the FraiseQL codebase, I can confirm that FraiseQL has excellent JSONB support, but your view structure needs adjustment to work with FraiseQL's conventions.

## The Issue

FraiseQL expects JSONB data to be in a column named `data`, but PrintOptim uses `json_data`. This naming mismatch is causing the query failures.

## FraiseQL's JSONB Architecture

1. **Built for JSONB**: FraiseQL is designed from the ground up to work with JSONB columns
2. **Expected Structure**: Tables/views should have:
   - A `data` column containing the JSONB object
   - Additional columns for commonly filtered fields
   - An `id` column for primary key

3. **Query Generation**: FraiseQL automatically generates:
   - JSONB path expressions (`data->>'field'`)
   - Type casting for comparisons
   - Complex WHERE clauses with JSONB operators

## Recommended Solution

Create FraiseQL-compatible wrapper views that:
1. Alias `json_data` as `data`
2. Extract commonly filtered fields as columns

### Example Implementation

```sql
-- Create a FraiseQL-compatible view
CREATE VIEW fraiseql.v_item_category AS
SELECT
    id,
    json_data as data,  -- CRITICAL: Rename to 'data'
    -- Extract fields used in WHERE clauses
    json_data->>'parent_id'::uuid as parent_id,
    json_data->>'name' as name,
    json_data->>'code' as code,
    (json_data->>'level')::int as level,
    (json_data->>'is_active')::boolean as is_active
FROM public.v_item_category;
```

### Updated Resolver

```python
@fraiseql.query
async def item_categories(
    info: GraphQLResolveInfo,
    limit: int = 50,
    offset: int = 0,
    parent_id: uuid.UUID | None = None,
) -> list[ItemCategory]:
    db = info.context["db"]

    where_clause = {}
    if parent_id is not None:
        where_clause["parent_id"] = {"eq": parent_id}

    return await db.find(
        "fraiseql.v_item_category",  # Use the new view
        where=where_clause,
        limit=limit,
        offset=offset,
        order_by="name ASC"
    )
```

## Why This Works

1. **Column Naming**: The `data` column name allows FraiseQL's internal mechanisms to work correctly
2. **Extracted Columns**: Fields like `parent_id` become real columns for efficient filtering
3. **JSONB Intact**: The full JSONB object remains available for complete data retrieval

## Alternative: Custom Repository

If you cannot modify views, extend FraiseQLRepository:

```python
class PrintOptimRepository(FraiseQLRepository):
    def _instantiate_from_row(self, type_class: type, row: dict[str, Any]) -> Any:
        # Handle both column names
        data = row.get("json_data", row.get("data"))
        return self._instantiate_recursive(type_class, data)
```

## Filtering Best Practices

1. **Extract frequently filtered fields** as view columns
2. **Use JSONB operators** for complex queries
3. **Index extracted columns** for performance

## Migration Path

1. Create new FraiseQL-compatible views alongside existing ones
2. Update resolvers to use new views
3. Test thoroughly with your integration suite
4. Consider creating a naming convention (e.g., `fraiseql.v_*` prefix)

## Key Takeaways

- FraiseQL fully supports JSONB, but expects a `data` column
- Views with `(id, json_data)` structure need simple aliasing
- Extract commonly filtered fields for better performance
- The framework handles all JSONB complexity internally

This approach maintains your existing database structure while providing full FraiseQL compatibility. The only change needed is creating wrapper views with proper column naming.
