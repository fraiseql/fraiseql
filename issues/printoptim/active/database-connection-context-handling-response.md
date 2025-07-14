# Response: Database Connection Handling in FraiseQL

## The Problem

You're experiencing connection lifecycle issues because you're trying to pass a raw psycopg connection in the context, but FraiseQL is designed to work with its `FraiseQLRepository` class, not raw connections.

## The Correct Pattern

### 1. Use FraiseQLRepository, Not Raw Connections

FraiseQL expects a `FraiseQLRepository` instance in the context, not a raw connection:

```python
from fraiseql.db import FraiseQLRepository

async def get_context(request: Request) -> dict[str, Any]:
    """Get context for GraphQL requests."""
    pool = get_db_pool(request)

    # Create repository with context
    context = {
        "mode": "development",  # or "production"
        "tenant_id": request.headers.get("tenant-id"),
    }

    # FraiseQLRepository manages its own connections
    repo = FraiseQLRepository(pool, context=context)

    return {
        "db": repo,  # Pass repository, not connection
        "request": request,
        "tenant_id": request.headers.get("tenant-id"),
        "contact_id": request.headers.get("contact-id"),
    }
```

### 2. Let FraiseQL Manage Connection Lifecycle

The `FraiseQLRepository` manages connections internally:

```python
# In your resolvers
@fraiseql.query
async def get_allocation(info, id: str) -> Optional[Allocation]:
    db: FraiseQLRepository = info.context["db"]
    # Repository handles connection acquisition/release
    return await db.find_one("tv_allocation", id=id)

@fraiseql.query
async def list_allocations(info, limit: int = 20) -> list[Allocation]:
    db: FraiseQLRepository = info.context["db"]
    # Each method call manages its own connection
    return await db.find("tv_allocation", limit=limit)
```

### 3. Use the Built-in Context Builder

The easiest approach is to use FraiseQL's built-in context handling:

```python
from fraiseql import create_fraiseql_app

app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[Allocation, Machine, Location],
    # Don't provide context_getter - use the default
)

# The default context builder provides:
# - db: FraiseQLRepository instance
# - user: UserContext (if auth is configured)
# - authenticated: bool
# - loader_registry: For DataLoader support
```

### 4. Custom Context with Proper Pattern

If you need custom context values, extend the default:

```python
from fraiseql.fastapi.dependencies import build_graphql_context
from fastapi import Request, Depends

async def get_custom_context(
    request: Request,
    default_context: dict = Depends(build_graphql_context)
) -> dict[str, Any]:
    """Extend default context with custom values."""
    # Add your custom values to the default context
    default_context.update({
        "tenant_id": request.headers.get("tenant-id"),
        "contact_id": request.headers.get("contact-id"),
        "custom_value": "something",
    })
    return default_context

# Use in app creation
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[Allocation, Machine, Location],
    context_getter=get_custom_context,
)
```

### 5. Multi-Tenant Pattern

For multi-tenancy, pass tenant info through the repository context:

```python
async def get_context(request: Request) -> dict[str, Any]:
    pool = get_db_pool()
    tenant_id = request.headers.get("tenant-id")

    # Pass tenant_id in repository context
    repo = FraiseQLRepository(pool, context={
        "tenant_id": tenant_id,
        "mode": "development",  # or from environment
    })

    return {
        "db": repo,
        "request": request,
        "tenant_id": tenant_id,  # Also available at top level if needed
    }
```

## Why Your Current Approach Fails

1. **Connection Scope**: `async with pool.connection()` closes the connection when the context manager exits
2. **No Connection Management**: Raw connections don't handle the async lifecycle properly
3. **Missing Features**: You lose FraiseQL's features like:
   - Automatic type instantiation from JSONB
   - Mode switching (dev/prod)
   - Built-in query methods

## Complete Working Example

```python
from fastapi import FastAPI, Request
from fraiseql import create_fraiseql_app, fraise_type
from fraiseql.db import FraiseQLRepository
import psycopg_pool

# Your types
@fraise_type
class Allocation:
    id: UUID
    identifier: str
    machine: Optional[Machine]
    # ... other fields

# Custom context getter
async def get_context(request: Request) -> dict[str, Any]:
    # Get pool from app state
    pool = request.app.state.db_pool

    # Create repository with request-specific context
    repo = FraiseQLRepository(pool, context={
        "tenant_id": request.headers.get("tenant-id"),
        "mode": "development",
    })

    return {
        "db": repo,
        "request": request,
        "tenant_id": request.headers.get("tenant-id"),
    }

# Create app
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[Allocation, Machine, Location],
    context_getter=get_context,
)

# Or even simpler - let FraiseQL handle everything
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[Allocation, Machine, Location],
    # No context_getter - use defaults
)
```

## Key Takeaways

1. **Use FraiseQLRepository**, not raw connections
2. **Let FraiseQL manage** the connection lifecycle
3. **Repository is stateless** - safe to create per request
4. **Each query method** manages its own connection
5. **No cleanup needed** - connections return to pool automatically

This pattern ensures proper connection management, prevents leaks, and gives you all of FraiseQL's features.
