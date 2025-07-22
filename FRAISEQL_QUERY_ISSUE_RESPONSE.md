# FraiseQL Query Issue - Response

## Root Cause Analysis

After investigating your issue, I've identified the root cause: **FraiseQL does not automatically generate query resolvers from type definitions**. This is by design - FraiseQL requires explicit query resolver definitions for each entity you want to query.

## Why Some Entities Work and Others Don't

The entities that work (DNS Server, Gateway) have query resolvers defined in your codebase, while the failing entities (Router, SMTP Server, Print Server, Network Configuration) are missing their query resolver definitions.

When you define a type with `@fraise_type`, it only registers the type for GraphQL schema generation. It does NOT create any queries automatically. You must explicitly define query resolvers using the `@fraiseql.query` decorator.

## Solution

Add the following query resolvers to your application code (typically in your GraphQL query module):

```python
import uuid
from graphql import GraphQLResolveInfo
import fraiseql

# Add query resolver for Router
@fraiseql.query
async def router(info: GraphQLResolveInfo, id: uuid.UUID) -> Router | None:
    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")
    return await db.find_one("v_router", id=id, tenant_id=tenant_id)

# Add query resolver for SMTP Server
@fraiseql.query
async def smtpServer(info: GraphQLResolveInfo, id: uuid.UUID) -> SmtpServer | None:
    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")
    return await db.find_one("v_smtp_server", id=id, tenant_id=tenant_id)

# Add query resolver for Print Server
@fraiseql.query
async def printServer(info: GraphQLResolveInfo, id: uuid.UUID) -> PrintServer | None:
    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")
    return await db.find_one("v_print_server", id=id, tenant_id=tenant_id)

# Add query resolver for Network Configuration
@fraiseql.query
async def networkConfiguration(info: GraphQLResolveInfo, id: uuid.UUID) -> NetworkConfiguration | None:
    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")
    return await db.find_one("v_network_configuration", id=id, tenant_id=tenant_id)
```

## Additional Query Resolvers (Optional)

You might also want to add list queries for these entities:

```python
# List queries for multiple entities
@fraiseql.query
async def routers(
    info: GraphQLResolveInfo, 
    limit: int = 100,
    offset: int = 0
) -> list[Router]:
    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")
    return await db.find("v_router", tenant_id=tenant_id, limit=limit, offset=offset)

@fraiseql.query
async def smtpServers(
    info: GraphQLResolveInfo,
    limit: int = 100,
    offset: int = 0
) -> list[SmtpServer]:
    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")
    return await db.find("v_smtp_server", tenant_id=tenant_id, limit=limit, offset=offset)

@fraiseql.query
async def printServers(
    info: GraphQLResolveInfo,
    limit: int = 100,
    offset: int = 0
) -> list[PrintServer]:
    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")
    return await db.find("v_print_server", tenant_id=tenant_id, limit=limit, offset=offset)

@fraiseql.query
async def networkConfigurations(
    info: GraphQLResolveInfo,
    limit: int = 100,
    offset: int = 0
) -> list[NetworkConfiguration]:
    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")
    return await db.find("v_network_configuration", tenant_id=tenant_id, limit=limit, offset=offset)
```

## Why This Happens

1. **Type Registration vs Query Registration**: The `@fraise_type` decorator only registers types for GraphQL schema generation. It doesn't create any queries.

2. **Explicit Design Choice**: FraiseQL follows an explicit approach where developers must define each query resolver. This provides more control over your API surface.

3. **No Auto-CRUD**: Unlike some GraphQL frameworks that automatically generate CRUD operations, FraiseQL requires manual definition of each query.

## Debugging Tips

To verify query registration, you can enable debug logging:

```python
import logging
logging.getLogger("fraiseql").setLevel(logging.DEBUG)
```

This will show you which queries are being registered when your application starts.

## Best Practices

1. **Organize Query Resolvers**: Keep all your query resolvers in a dedicated module (e.g., `queries.py` or `graphql/queries/`)

2. **Consistent Naming**: Follow GraphQL conventions:
   - Singular queries for single items: `router(id: ID!)`
   - Plural queries for lists: `routers(limit: Int, offset: Int)`

3. **Import Order**: Ensure your query resolver modules are imported before creating the FraiseQL app:
   ```python
   # Import query resolvers first
   import your_app.graphql.queries
   
   # Then create the app
   app = create_fraiseql_app(...)
   ```

## Summary

The "Internal server error" occurs because FraiseQL can't find query resolvers for your entities. Simply add the query resolver functions shown above, and your GraphQL queries will work correctly. The mutations work because they're explicitly defined, but queries need the same explicit definition.

This is working as designed - FraiseQL gives you full control over your GraphQL API by requiring explicit resolver definitions rather than auto-generating them.