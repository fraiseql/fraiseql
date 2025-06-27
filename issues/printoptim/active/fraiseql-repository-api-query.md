# FraiseQLRepository API for Query Functions

## Issue

We've successfully converted to function-based queries using `@fraiseql.query` decorators as instructed. The queries are being discovered and registered correctly, and the GraphQL schema shows all our query fields (`machine`, `machines`, `allocation`, `allocations`).

However, when trying to use the database repository in our query functions, we're getting an error:

```
'FraiseQLRepository' object has no attribute 'find'
```

## Current Query Implementation

```python
@fraiseql.query
async def machines(
    info,
    limit: int = 20,
    offset: int = 0,
    where: MachineWhereInput | None = None,
) -> list[Machine]:
    """Retrieve a list of machines."""
    db = info.context["db"]  # This is a FraiseQLRepository
    tenant_id = info.context.get("tenant_id", "550e8400-e29b-41d4-a716-446655440000")

    # This line fails - 'find' method doesn't exist
    return await db.find("tv_machine",
        tenant_id=tenant_id,
        limit=limit,
        offset=offset,
        order_by="removed_at DESC NULLS LAST"
    )
```

## Error Details

- The GraphQL endpoint responds correctly
- The `info.context["db"]` contains a `FraiseQLRepository` object
- But the repository doesn't have a `find` method

## Questions

1. What is the correct API for `FraiseQLRepository` in v0.1.0a14?
2. What methods are available for querying data from database views?
3. Should we be using a different approach to access data in query functions?
4. Is there updated documentation for the repository API?

## Context

- Using FraiseQL v0.1.0a14
- Database views with JSONB `data` columns (e.g., `tv_machine`, `tv_allocation`)
- Successfully migrated from class-based to function-based queries
- Need to query data and have FraiseQL automatically instantiate objects from JSONB

## Expected Behavior

Based on previous guidance, we expected to be able to:
1. Use `await db.find(table_name, **filters)` to query multiple records
2. Use `await db.find_one(table_name, **filters)` to query single records
3. Have FraiseQL automatically instantiate our type classes from JSONB data

Please provide the correct API documentation or examples for using the repository in query functions.
