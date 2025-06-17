# Query Registration in FraiseQL

FraiseQL supports multiple patterns for registering GraphQL queries. This guide explains each pattern and helps you choose the best one for your use case.

## TL;DR - Recommended Pattern

Use the `@fraiseql.query` decorator for the cleanest API:

```python
import fraiseql
from fraiseql.fastapi import create_fraiseql_app

# Define your types
@fraiseql.type
class User:
    id: UUID
    name: str
    email: str

# Define queries with @query decorator - they auto-register!
@fraiseql.query
async def get_user(info, id: UUID) -> User:
    db = info.context["db"]
    return await db.get_user(id)

@fraiseql.query
async def list_users(info) -> list[User]:
    db = info.context["db"]
    return await db.get_all_users()

# Create app - no need to list queries!
app = create_fraiseql_app(
    database_url="postgresql://localhost/myapp",
    types=[User]  # Just list your types
)
```

## Important: Import Your Query Modules!

The `@query` decorator registers functions when the module is imported. Make sure to import your query modules before creating the app:

```python
# ✅ CORRECT - Import queries module
import queries  # This executes the @query decorators

# ❌ WRONG - Queries not imported
# from queries import get_user  # This would work but defeats the purpose

app = create_fraiseql_app(...)
```

## All Query Registration Patterns

### 1. @fraiseql.query Decorator (Recommended)

**Pros:**
- Clean, declarative syntax
- Auto-registration at import time
- No need to maintain a separate list
- Works like Flask/FastAPI routes

**Example:**
```python
@fraiseql.query
async def get_post(info, id: UUID) -> Post:
    """Get a blog post by ID."""
    return await info.context["db"].get_post(id)

@fraiseql.query
@requires_auth  # Can combine with other decorators
async def my_posts(info) -> list[Post]:
    """Get current user's posts."""
    user = info.context["user"]
    return await info.context["db"].get_user_posts(user.id)
```

### 2. QueryRoot Class with @field

**Pros:**
- Groups related queries in a class
- Good for organizing complex APIs
- Supports computed properties

**Example:**
```python
@fraiseql.type
class QueryRoot:
    """Root query type."""

    @fraiseql.field(description="API version")
    def version(self, root, info) -> str:
        return "1.0.0"

    @fraiseql.field
    async def stats(self, root, info) -> dict[str, int]:
        db = info.context["db"]
        return await db.get_stats()

# Include QueryRoot in types
app = create_fraiseql_app(
    types=[User, Post, QueryRoot]  # Include QueryRoot here
)
```

### 3. Explicit Function Registration

**Pros:**
- Full control over what's registered
- Good for conditional registration
- Useful for third-party functions

**Example:**
```python
# Regular async function (no decorator)
async def search_posts(info, query: str) -> list[Post]:
    return await info.context["db"].search_posts(query)

# Conditionally registered query
if FEATURE_FLAG_ENABLED:
    async def experimental_query(info) -> str:
        return "experimental"

# Register explicitly
app = create_fraiseql_app(
    types=[User, Post],
    queries=[
        search_posts,
        experimental_query if FEATURE_FLAG_ENABLED else None
    ]
)
```

## Mixing Patterns

You can use all three patterns together:

```python
import fraiseql
from fraiseql.fastapi import create_fraiseql_app

# Pattern 1: Auto-registered queries
@fraiseql.query
async def get_user(info, id: UUID) -> User:
    pass

# Pattern 2: QueryRoot with fields
@fraiseql.type
class QueryRoot:
    @fraiseql.field
    def api_version(self, root, info) -> str:
        return "1.0.0"

# Pattern 3: Explicit function
async def search_users(info, query: str) -> list[User]:
    pass

# All patterns work together!
app = create_fraiseql_app(
    types=[User, QueryRoot],  # QueryRoot goes in types
    queries=[search_users]    # Only explicit functions here
    # @query decorated functions are auto-included
)
```

## Common Pitfalls

### 1. Forgetting to Import Query Modules

```python
# ❌ WRONG - queries.py never imported
app = create_fraiseql_app(types=[User])
# Result: "Type Query must define one or more fields"

# ✅ CORRECT - Import the module
import queries
app = create_fraiseql_app(types=[User])
```

### 2. Mixing Up Parameters

```python
# ❌ WRONG - QueryRoot in queries parameter
app = create_fraiseql_app(
    queries=[QueryRoot]  # Should be in types!
)

# ✅ CORRECT - QueryRoot in types parameter
app = create_fraiseql_app(
    types=[QueryRoot]
)
```

### 3. Circular Imports

```python
# ❌ WRONG - app.py imports queries.py which imports app.py
# queries.py
from app import app  # Circular!

# ✅ CORRECT - Use dependency injection
# queries.py
@fraiseql.query
async def get_user(info, id: UUID) -> User:
    db = info.context["db"]  # Get from context
```

## Migration from Function Lists

If you have existing code that passes functions explicitly:

```python
# Old pattern
app = create_fraiseql_app(
    queries=[get_user, get_posts, search_posts]
)
```

Simply add `@fraiseql.query` decorators:

```python
# New pattern
@fraiseql.query
async def get_user(info, id: UUID) -> User:
    ...

@fraiseql.query
async def get_posts(info) -> list[Post]:
    ...

@fraiseql.query
async def search_posts(info, query: str) -> list[Post]:
    ...

# Now just import and create app
import queries  # Executes decorators
app = create_fraiseql_app(types=[User, Post])
```

## Best Practices

1. **Use @query for most queries** - It's the cleanest pattern
2. **Use QueryRoot for grouped functionality** - Like API metadata
3. **Use explicit registration for dynamic queries** - When you need conditions
4. **Always import query modules** - Before creating the app
5. **Keep queries in separate modules** - Avoid circular imports
6. **Add type hints** - FraiseQL uses them to generate the schema

## Debugging Tips

If you get "Type Query must define one or more fields":

1. Check that query modules are imported
2. Verify @query decorators are applied
3. Ensure return type annotations exist
4. Check that registry isn't being cleared after decoration

```python
# Debug: List registered queries
from fraiseql.gql.schema_builder import SchemaRegistry
registry = SchemaRegistry.get_instance()
print("Registered queries:", list(registry._queries.keys()))
```
