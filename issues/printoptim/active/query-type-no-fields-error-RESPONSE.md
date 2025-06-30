# Response: Query Type No Fields Error

## The Problem

You're trying to use a class-based `Query` type, but **FraiseQL doesn't use resolver classes**. This is the fundamental issue causing your error.

## The Solution

Use the `@fraiseql.query` decorator on standalone functions instead of methods in a class.

### ❌ WRONG: Class-Based Approach (What You Have)
```python
@fraiseql.type
class Query:
    """This doesn't work in FraiseQL!"""

    async def machines(self, info, ...):
        pass
```

### ✅ CORRECT: Function-Based Approach (What You Need)
```python
# queries.py
import fraiseql
from uuid import UUID

@fraiseql.query
async def machines(
    info,
    limit: int = 20,
    offset: int = 0,
    where: MachineWhereInput | None = None
) -> list[Machine]:
    """Retrieve a list of machines."""
    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")

    return await db.find("tv_machine",
        tenant_id=tenant_id,
        limit=limit,
        offset=offset,
        order_by="removed_at DESC NULLS LAST"
    )

@fraiseql.query
async def machine(info, id: UUID) -> Machine | None:
    """Get a single machine by ID."""
    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")

    return await db.find_one("tv_machine",
        id=id,
        tenant_id=tenant_id
    )
```

## App Creation

Remove the `queries` parameter entirely - FraiseQL auto-discovers queries:

### ❌ WRONG: Passing Query Classes
```python
QUERIES: list[type] = [Query]  # Remove this!

fraiseql_app = create_fraiseql_app(
    config=fraiseql_config,
    types=TYPES,
    queries=QUERIES,  # Remove this parameter!
    mutations=MUTATIONS,
    context_getter=get_context,
)
```

### ✅ CORRECT: Let FraiseQL Auto-Discover
```python
# Make sure to import your queries module so decorators register
import your_app.queries  # This registers all @fraiseql.query functions

fraiseql_app = create_fraiseql_app(
    config=fraiseql_config,
    types=TYPES,  # Your type definitions (Machine, etc.)
    # No queries parameter! They're auto-discovered
    mutations=MUTATIONS,
    context_getter=get_context,
)
```

## Complete Working Example

### 1. Define Your Types (`types.py`)
```python
from fraiseql import fraise_type
from uuid import UUID
from datetime import datetime

@fraise_type
class Machine:
    id: UUID
    name: str
    status: str
    created_at: datetime
    removed_at: datetime | None = None

@fraise_input
class MachineWhereInput:
    status: str | None = None
    name_contains: str | None = None
```

### 2. Define Your Queries (`queries.py`)
```python
import fraiseql
from uuid import UUID
from .types import Machine, MachineWhereInput

@fraiseql.query
async def machines(
    info,
    limit: int = 20,
    offset: int = 0,
    where: MachineWhereInput | None = None
) -> list[Machine]:
    """Retrieve a list of machines."""
    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")

    filters = {"tenant_id": tenant_id} if tenant_id else {}

    # Add where conditions if provided
    if where:
        if where.status:
            filters["status"] = where.status
        # Add other filters as needed

    return await db.find("tv_machine",
        **filters,
        limit=limit,
        offset=offset,
        order_by="removed_at DESC NULLS LAST"
    )

@fraiseql.query
async def machine(info, id: UUID) -> Machine | None:
    """Get a single machine by ID."""
    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")

    filters = {"id": id}
    if tenant_id:
        filters["tenant_id"] = tenant_id

    return await db.find_one("tv_machine", **filters)

@fraiseql.query
async def machine_count(info) -> int:
    """Get total count of machines."""
    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")

    # You'd need to implement a count method or use raw SQL
    result = await db.find("tv_machine", tenant_id=tenant_id)
    return len(result)
```

### 3. Create Your App (`app.py`)
```python
from fraiseql import create_fraiseql_app
from .types import Machine, MachineWhereInput  # Import types
from . import queries  # Import queries module to register them

# Your types list
TYPES = [Machine, MachineWhereInput]

# Create app - queries are auto-discovered!
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=TYPES,
    context_getter=get_context,
    production=False  # Enable GraphQL playground
)
```

## Key Points to Remember

1. **No Query class** - FraiseQL doesn't use resolver classes
2. **Use @fraiseql.query** - This decorator registers your queries
3. **Functions, not methods** - Queries are standalone functions
4. **info is first** - The first parameter is always `info`
5. **Auto-discovery** - Just import your queries module
6. **Return types required** - Always add type annotations

## Testing Your Queries

Visit `http://localhost:8000/graphql` and try:

```graphql
query {
  machines(limit: 10) {
    id
    name
    status
    createdAt
    removedAt
  }

  machine(id: "123e4567-e89b-12d3-a456-426614174000") {
    id
    name
    status
  }

  machineCount
}
```

## Migration Checklist

- [ ] Remove the `Query` class entirely
- [ ] Convert methods to standalone functions with `@fraiseql.query`
- [ ] Ensure `info` is the first parameter
- [ ] Add return type annotations
- [ ] Import your queries module in your app
- [ ] Remove `queries` parameter from `create_fraiseql_app`
- [ ] Test that queries appear in GraphQL schema

## Need More Help?

See the [Query Patterns Documentation](https://github.com/fraiseql/fraiseql/blob/main/docs/QUERY_PATTERNS.md) for more examples and patterns.
