# @query Decorator

The `@query` decorator provides a simple way to register GraphQL queries without creating a QueryRoot class.

## Basic Usage

```python
from fraiseql import query
from typing import Optional
from uuid import UUID

@query
async def get_user(info, id: UUID) -> Optional[User]:
    """Get a user by ID."""
    db = info.context["db"]
    user_data = await db.fetch_one(
        "SELECT * FROM users WHERE id = $1",
        id
    )
    return User(**user_data) if user_data else None

@query
async def list_users(info, limit: int = 10) -> list[User]:
    """List all users with pagination."""
    db = info.context["db"]
    users_data = await db.fetch_all(
        "SELECT * FROM users LIMIT $1",
        limit
    )
    return [User(**data) for data in users_data]
```

## How It Works

The `@query` decorator:
1. Marks a function as a GraphQL query field
2. Automatically registers it with the schema builder
3. Handles the GraphQL resolver signature conversion

## When to Use @query vs QueryRoot

### Use `@query` when:
- You prefer simple function-based queries
- You want automatic registration
- You're migrating from other GraphQL libraries
- You have many standalone query functions

### Use QueryRoot when:
- You need complex field dependencies
- You want to group related queries
- You prefer class-based organization
- You need custom field resolvers

## Complete Example

```python
from fraiseql import query, create_fraiseql_app
from models import User, Post

@query
async def me(info) -> Optional[User]:
    """Get current authenticated user."""
    user_context = info.context.get("user")
    if not user_context:
        return None

    db = info.context["db"]
    return await db.get_user(user_context.user_id)

@query
async def search_posts(
    info,
    query: str,
    limit: int = 20,
    offset: int = 0
) -> list[Post]:
    """Search posts by title or content."""
    db = info.context["db"]
    return await db.search_posts(query, limit, offset)

# Create app - queries are auto-registered
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User, Post],
    # No need to pass queries - they're auto-registered!
)
```

## Advanced Usage

### With Authentication

```python
from fraiseql import query, requires_auth

@query
@requires_auth
async def private_data(info) -> dict[str, Any]:
    """Access private data - requires authentication."""
    user = info.context["user"]
    return {
        "user_id": user.user_id,
        "secret": "Only authenticated users can see this"
    }
```

### With Custom Context

```python
@query
async def with_custom_context(info) -> dict[str, Any]:
    """Use custom context data."""
    return {
        "db_pool_size": info.context["db"].pool.size,
        "request_id": info.context.get("request_id"),
        "features": info.context.get("features", {})
    }
```

## Implementation Details

The `@query` decorator:
- Stores the function in a global registry
- Preserves the original function signature
- Adds proper GraphQL metadata
- Integrates with FraiseQL's schema builder

## See Also

- [Field Decorator](./field-decorator.md) - For class-based queries
- [Schema Building](../schema-building.md) - How queries are registered
- [Context](../advanced/context-customization.md) - Working with GraphQL context
