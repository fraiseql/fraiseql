# Database Context Missing in FraiseQL v0.1.0a15

## Issue Description

After upgrading from FraiseQL v0.1.0a14 to v0.1.0a15, the database repository is no longer accessible in the query context. Our queries that previously worked are now failing with `KeyError: 'db'`.

## Environment

- FraiseQL version: 0.1.0a15 (upgraded from 0.1.0a14)
- Python version: 3.13
- Framework: FastAPI with FraiseQL

## Current Implementation

### App Setup (app.py)
```python
# Create config without loading environment variables
fraiseql_config = FraiseQLConfig(
    _env_file=None,  # Disable .env file loading
    database_url=settings.database_url,  # PostgreSQL connection string
    environment="development",
    enable_introspection=True,
    enable_playground=True,
    playground_tool="apollo-sandbox",
    auth_enabled=False,
)

# Custom context getter to add tenant_id
async def get_context(request):
    """Get context with tenant_id from headers."""
    return {
        "tenant_id": request.headers.get("tenant-id", "550e8400-e29b-41d4-a716-446655440000"),
        "contact_id": request.headers.get("contact-id"),
    }

# Create FraiseQL app
fraiseql_app = create_fraiseql_app(
    config=fraiseql_config,
    types=TYPES,
    mutations=MUTATIONS,
    context_getter=get_context,
)
```

### Query Implementation (queries.py)
```python
@fraiseql.query
async def machines(
    info,
    limit: int = 20,
    offset: int = 0,
    where: MachineWhereInput | None = None,
) -> list[Machine]:
    """Retrieve a list of machines with filtering."""

    db = info.context["db"]  # <-- This line fails with KeyError: 'db'
    tenant_id = info.context.get("tenant_id", "550e8400-e29b-41d4-a716-446655440000")

    # Build filters from where input
    filters = _build_machine_filters(where, tenant_id)

    return await db.find("tb_machine",
        **filters,
        limit=limit,
        offset=offset,
        order_by="removed_at DESC NULLS LAST"
    )
```

## Error Details

When running any query that tries to access the database:

```graphql
query {
  machines(limit: 3) {
    id
    identifier
  }
}
```

Response:
```json
{
  "data": {
    "machines": null
  },
  "errors": [
    {
      "message": "'db'",
      "locations": [{"line": 1, "column": 21}],
      "path": ["machines"],
      "extensions": {}
    }
  ]
}
```

## Debug Information

I created a debug query to inspect the context:

```python
@fraiseql.query
async def debug_context(info) -> str:
    """Debug query to check what's in the context."""
    context_keys = list(info.context.keys())
    return f"Context type: {type(info.context)}\nContext keys: {context_keys}"
```

Result:
```
Context type: <class 'dict'>
Context keys: ['tenant_id', 'contact_id', 'n1_detector']
  tenant_id: str
```

The context only contains our custom fields but no database connection.

## What Changed?

In v0.1.0a14, the database repository was automatically injected into the context under the key `"db"`. This appears to have changed in v0.1.0a15.

## Questions

1. **How should we access the FraiseQLRepository in v0.1.0a15?**
   - Is it under a different key in the context?
   - Is it available as a property on the `info` object?
   - Do we need to modify our context_getter to include it?

2. **Is there a migration guide for v0.1.0a14 to v0.1.0a15?**
   - We couldn't find a changelog entry for v0.1.0a15

3. **What is the correct pattern for accessing the database in queries?**
   - Should we be using a different approach than `info.context["db"]`?

## Expected Behavior

The FraiseQLRepository should be accessible in query resolvers to perform database operations, as it was in v0.1.0a14.

## Temporary Workaround Needed

We need to know how to access the database repository in our queries with v0.1.0a15 so we can continue development. Any guidance on the new pattern would be greatly appreciated.

## Additional Context

- All queries are decorated with `@fraiseql.query`
- We're using the operator-based where types as recommended
- The GraphQL endpoint itself is working (introspection queries work)
- Only database access is failing

Please advise on the correct way to access the database repository in FraiseQL v0.1.0a15.
