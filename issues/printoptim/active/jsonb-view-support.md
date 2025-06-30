# JSONB View Query Issues in PrintOptim

## Issue Summary
When querying PostgreSQL views that store data in JSONB columns, FraiseQL queries are failing with column not found errors. Need guidance on the correct approach for querying JSONB-based views.

## Current Situation

### Database Structure
PrintOptim has views like `public.v_item_category`:
```sql
\d public.v_item_category
           View "public.v_item_category"
  Column   | Type  | Collation | Nullable | Default
-----------+-------+-----------+----------+---------
 id        | uuid  |           |          |
 json_data | jsonb |           |          |
```

Sample data:
```sql
SELECT * FROM public.v_item_category LIMIT 1;
                  id                  |                                     json_data
--------------------------------------+-----------------------------------------------------------------------------------
 a091f7e9-50d1-4fc6-aee5-f8745740abdb | {"id": "a091f7e9-50d1-4fc6-aee5-f8745740abdb", "name": "équipement", "parent_id": null, "identifier": "equipment"}
```

### GraphQL Type
```python
@fraiseql.type
class ItemCategory(BaseGQLType):
    id: uuid.UUID
    name: str
    code: str
    parent_id: uuid.UUID | None = None
    level: int = 0
    is_active: bool = True
```

### Resolver Implementation
```python
@fraiseql.query
async def item_categories(
    info: GraphQLResolveInfo,
    limit: int = 50,
    offset: int = 0,
    parent_id: uuid.UUID | None = None,
) -> list[ItemCategory]:
    db = info.context["db"]  # FraiseQLRepository

    where_clause = None
    if parent_id is not None:
        where_clause = {"parent_id": {"eq": parent_id}}

    return await db.find(
        "v_item_category",
        where=where_clause,
        limit=limit,
        offset=offset,
        order_by="name ASC"
    )
```

## Error Encountered
When querying:
```graphql
{ itemCategories(limit: 5) { id name code parentId level isActive } }
```

Error:
```
psycopg.errors.UndefinedColumn: column "tenant_id" does not exist
LINE 1: SELECT * FROM "v_item_category" WHERE tenant_id = $1 ORDER B...
```

After removing tenant_id, subsequent errors suggest FraiseQL might be looking for direct columns that don't exist in the view.

## Questions

1. **What is the correct way to query views with JSONB data?**
   - Should the view structure be different?
   - Is there a special configuration needed for JSONB views?
   - Should we handle the mapping differently in the resolver?

2. **How should filtering work on JSONB fields?**
   - For example, filtering by `parent_id` which is inside the `json_data` column
   - Should we use PostgreSQL JSONB operators like `json_data->>'parent_id'`?

3. **Is there a recommended view structure for FraiseQL?**
   - Should fields be expanded as direct columns?
   - Or is JSONB the expected format?

## Additional Context
- This pattern is used throughout PrintOptim's database
- Many views follow the `(id, json_data)` structure
- Integration tests are blocked until we understand the correct approach

## Request
Please provide guidance on:
1. The correct way to structure views for FraiseQL
2. How to properly query JSONB-based views
3. Any examples or documentation for this use case

Thank you!
