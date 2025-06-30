# Response: Resolver Info Parameter is None - FraiseQL Query Patterns

## The Problem

You're mixing traditional GraphQL resolver patterns with FraiseQL's simplified approach. FraiseQL doesn't use `resolve_*` prefix methods or two-tier resolver structures.

## FraiseQL Query Patterns

### Pattern 1: @fraiseql.query Decorator (RECOMMENDED)

This is the simplest and most direct approach:

```python
import fraiseql
from uuid import UUID
from typing import Optional

@fraiseql.query
async def machines(
    info,  # GraphQL resolve info is first parameter
    limit: int = 20,
    offset: int = 0,
    where: Optional[MachineWhereInput] = None,
) -> list[Machine]:
    """Retrieve a list of machines."""
    db = info.context["db"]  # Access FraiseQLRepository
    tenant_id = info.context.get("tenant_id")

    # Use FraiseQL's find method
    return await db.find("tv_machine",
        tenant_id=tenant_id,
        limit=limit,
        offset=offset
    )

@fraiseql.query
async def machine(info, id: UUID) -> Optional[Machine]:
    """Get a single machine by ID."""
    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")

    return await db.find_one("tv_machine",
        id=id,
        tenant_id=tenant_id
    )
```

### Pattern 2: QueryRoot Class (If You Need Grouping)

Use this if you prefer organizing queries in a class:

```python
@fraiseql.type
class QueryRoot:
    """Root query type."""

    @fraiseql.field
    async def machines(
        self,
        root,  # Always None for root queries
        info,  # GraphQL resolve info
        limit: int = 20,
        offset: int = 0,
        where: Optional[MachineWhereInput] = None,
    ) -> list[Machine]:
        """Retrieve a list of machines."""
        db = info.context["db"]
        tenant_id = info.context.get("tenant_id")

        return await db.find("tv_machine",
            tenant_id=tenant_id,
            limit=limit,
            offset=offset
        )

    @fraiseql.field
    async def machine(self, root, info, id: UUID) -> Optional[Machine]:
        """Get a single machine by ID."""
        db = info.context["db"]
        return await db.find_one("tv_machine", id=id)
```

## Key Differences from Traditional GraphQL

1. **No resolve_ prefix**: Use the actual field name
2. **No two-tier structure**: Put logic directly in the query function
3. **Use @fraiseql.field**: Not resolver methods
4. **Parameter order matters**:
   - For @query: `(info, ...args)`
   - For @field: `(self, root, info, ...args)`

## Fixing Your Code

### 1. Remove the Two-Tier Structure

Instead of:
```python
# DON'T DO THIS
@fraiseql.type
class QueryRoot:
    async def resolve_machines(self, info, ...):
        return await gql_mat_query.machines(info, ...)  # Two-tier
```

Do this:
```python
# DO THIS - Direct implementation
@fraiseql.query
async def machines(info, limit: int = 20, ...) -> list[Machine]:
    db = info.context["db"]
    return await db.find("tv_machine", limit=limit)
```

### 2. Update Your App Creation

```python
# If using @fraiseql.query decorator
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[Machine, Location, Allocation],
    # No need to pass queries - decorator handles it
)

# If using QueryRoot class
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[Machine, Location, Allocation],
    queries=[QueryRoot],  # Pass the class
)
```

### 3. Complete Working Example

```python
# models.py
@fraiseql.type
class Machine:
    id: UUID
    identifier: str
    machine_serial_number: str
    # ... other fields

# queries.py
@fraiseql.query
async def machines(
    info,
    limit: int = 20,
    offset: int = 0,
    where: Optional[MachineWhereInput] = None,
) -> list[Machine]:
    """Retrieve machines with filtering."""
    db: FraiseQLRepository = info.context["db"]
    tenant_id = info.context.get("tenant_id")

    # Build filters
    filters = {"tenant_id": tenant_id}
    if where:
        filters.update(vars(where))

    return await db.find("tv_machine", **filters, limit=limit, offset=offset)

@fraiseql.query
async def machine(info, id: UUID) -> Optional[Machine]:
    """Get single machine."""
    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")

    return await db.find_one("tv_machine",
        id=id,
        tenant_id=tenant_id
    )

# main.py
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[Machine, Location, Allocation],
    context_getter=get_context,  # Your custom context
)
```

## Context Access

The context is available via `info.context`:

```python
@fraiseql.query
async def my_query(info, ...):
    # Access context values
    db = info.context["db"]              # FraiseQLRepository
    tenant_id = info.context["tenant_id"] # From your context_getter
    user = info.context.get("user")      # If auth is configured
    request = info.context["request"]     # FastAPI Request object
```

## File Structure Recommendation

```
resolvers/
  queries.py          # All @fraiseql.query decorated functions
  mutations.py        # All @fraiseql.mutation decorated functions
models/
  machine.py          # @fraiseql.type definitions
  allocation.py
```

Keep it simple - no need for complex hierarchies or separate resolver modules.

## Summary

1. **Use @fraiseql.query** decorator for queries
2. **First parameter is always `info`** (not self, not None)
3. **No resolve_ prefix** - use actual field names
4. **Put logic directly in query functions** - no two-tier pattern
5. **Access context via `info.context`**

This simpler pattern is why FraiseQL exists - to remove the boilerplate of traditional GraphQL!
