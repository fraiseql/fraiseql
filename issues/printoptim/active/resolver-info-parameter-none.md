# Resolver Info Parameter is None

## Issue Description

After following the guidance to let FraiseQL handle database connections, we're encountering an issue where the `info` parameter passed to our resolvers is `None`.

## Current Setup

### 1. App Configuration
```python
# Create FraiseQL app (let it create a new FastAPI instance)
fraiseql_app = create_fraiseql_app(
    config=fraiseql_config,
    types=TYPES,
    queries=QUERIES,
    mutations=MUTATIONS,
    # Don't pass our app - let FraiseQL create one
)
```

### 2. Query Type Definition
```python
@fraiseql.type
class QueryRoot:
    """Root query type for GraphQL schema."""

    async def resolve_machines(
        self,
        info,
        limit: int = 20,
        offset: int = 0,
        where: MachineWhereInput | None = None,
    ) -> list[Machine]:
        """Retrieve a list of machines."""
        filters = vars(where) if where else None
        return await gql_mat_query.machines(info, limit, offset, filters)
```

### 3. Resolver Implementation
```python
async def machines(
    info: GraphQLResolveInfo,
    limit: int = 20,
    offset: int = 0,
    where: dict[str, Any] | None = None,
) -> list[Machine]:
    """Retrieve a list of machines based on optional filtering."""
    # Debug output shows:
    # DEBUG: info type: <class 'NoneType'>
    # DEBUG: info: None

    conn = info.context["db"]  # This fails with AttributeError
    tenant_id = info.context.get("tenant_id")
    # ... rest of implementation
```

## Error

When executing a GraphQL query:
```graphql
query TestMachines {
  machines {
    id
    identifier
    machineSerialNumber
  }
}
```

We get:
```json
{
  "errors": [{
    "message": "'NoneType' object has no attribute 'context'",
    "path": ["machines"]
  }]
}
```

## Questions

1. **Resolver Naming**: Should resolver methods be named with or without the `resolve_` prefix?
   - We have: `async def resolve_machines(self, info, ...)`
   - Should it be: `async def machines(self, info, ...)`?

2. **Info Parameter**: How is the `info` parameter supposed to be passed to resolvers in FraiseQL?
   - Is it automatically injected by FraiseQL?
   - Do we need to use a specific decorator?

3. **Resolver Structure**: We have a two-tier structure:
   - QueryRoot class with `resolve_*` methods
   - Separate resolver functions in query modules

   Is this the correct pattern, or should all resolver logic be in the QueryRoot class methods?

4. **Context Access**: Once we fix the info parameter issue, how should we access:
   - The database (previously `info.context["db"]`)
   - Request headers like tenant_id
   - Other context values

## Current Code Structure

```
resolvers/
  query/
    query.py          # Contains QueryRoot class with resolve_* methods
    dim/
      mat/
        gql_mat_query.py  # Contains actual resolver functions
```

Should we consolidate these or keep them separate? What's the recommended pattern for FraiseQL?

Please provide guidance on the correct way to structure resolvers for FraiseQL v0.1.0a14.
