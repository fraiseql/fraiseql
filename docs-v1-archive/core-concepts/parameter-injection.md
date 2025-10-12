# Parameter Injection Guide

**Understanding how GraphQL arguments map to Python function parameters in FraiseQL.**

## Overview

FraiseQL automatically handles the mapping between GraphQL query arguments and Python function parameters. Understanding this mechanism is crucial for writing correct resolvers and avoiding common errors.

## The `info` Parameter

### What is `info`?

The `info` parameter is **automatically injected** by FraiseQL into every query and mutation resolver. It provides access to:

- **Context**: Database connection, user authentication, request data
- **Field information**: Field name, parent type, return type
- **GraphQL metadata**: Operation name, variables, fragments

### Automatic Injection

The `info` parameter is **always the first parameter** in resolver functions, but it's **not part of the GraphQL schema**. FraiseQL injects it automatically.

```python
from fraiseql import query

@query
async def users(info, limit: int = 10) -> list[User]:
    """Get users with pagination."""
    repo = info.context["repo"]
    return await repo.find("v_user", limit=limit)
```

**GraphQL Schema Generated:**
```graphql
type Query {
  users(limit: Int = 10): [User!]!
  # Note: 'info' is NOT in the schema - it's injected automatically
}
```

### Accessing Context

The most common use of `info` is accessing the context:

```python
@query
async def my_profile(info) -> User | None:
    """Get current user's profile."""
    # Access database repository
    repo = info.context["repo"]

    # Access authenticated user
    user_context = info.context.get("user")
    if not user_context:
        return None

    # Access custom context
    request = info.context.get("request")
    tenant_id = info.context.get("tenant_id")

    return await repo.find_one("v_user", id=user_context.user_id)
```

## GraphQL Arguments → Python Parameters

### Basic Mapping

GraphQL arguments are mapped to Python function parameters **by name**. The types are automatically converted.

```python
@query
async def user(info, id: UUID) -> User | None:
    """Get user by ID."""
    repo = info.context["repo"]
    return await repo.find_one("v_user", id=id)
```

**GraphQL Query:**
```graphql
query {
  user(id: "550e8400-e29b-41d4-a716-446655440000") {
    id
    name
  }
}
```

**Parameter Flow:**
1. GraphQL receives `id: "550e8400-e29b-41d4-a716-446655440000"`
2. FraiseQL converts string to `UUID` type
3. Python function receives `id` as `UUID` object

### Optional Parameters with Defaults

Python default values become GraphQL optional arguments:

```python
@query
async def search_users(
    info,
    name: str | None = None,
    limit: int = 10,
    offset: int = 0
) -> list[User]:
    """Search users with optional filters."""
    repo = info.context["repo"]

    filters = {}
    if name:
        filters["name__icontains"] = name

    return await repo.find("v_user", where=filters, limit=limit, offset=offset)
```

**GraphQL Schema:**
```graphql
type Query {
  searchUsers(
    name: String
    limit: Int = 10
    offset: Int = 0
  ): [User!]!
}
```

**Valid Queries:**
```graphql
# All parameters optional
{ searchUsers { name } }

# Some parameters provided
{ searchUsers(name: "John") { name } }

# Override defaults
{ searchUsers(limit: 50, offset: 100) { name } }

# All parameters
{ searchUsers(name: "John", limit: 5, offset: 0) { name } }
```

### Input Types

For complex arguments, use input types:

```python
from fraiseql import fraise_input, query

@fraise_input
class SearchUsersInput:
    name: str | None = None
    email: str | None = None
    min_age: int | None = None
    max_age: int | None = None

@query
async def search_users(info, filters: SearchUsersInput) -> list[User]:
    """Search users with complex filters."""
    repo = info.context["repo"]

    where = {}
    if filters.name:
        where["name__icontains"] = filters.name
    if filters.email:
        where["email"] = filters.email
    if filters.min_age:
        where["age__gte"] = filters.min_age
    if filters.max_age:
        where["age__lte"] = filters.max_age

    return await repo.find("v_user", where=where)
```

**GraphQL Query:**
```graphql
query {
  searchUsers(filters: {
    name: "John"
    minAge: 18
    maxAge: 65
  }) {
    id
    name
    age
  }
}
```

## Common Patterns

### 1. Pagination Pattern

```python
@query
async def users_paginated(
    info,
    limit: int = 20,
    offset: int = 0,
    order_by: str = "created_at"
) -> list[User]:
    """Paginated user listing."""
    repo = info.context["repo"]
    return await repo.find(
        "v_user",
        limit=limit,
        offset=offset,
        order_by=[(order_by, "DESC")]
    )
```

### 2. Filter Pattern

```python
@query
async def posts(
    info,
    author_id: UUID | None = None,
    status: str | None = None,
    published: bool | None = None
) -> list[Post]:
    """Filter posts by multiple criteria."""
    repo = info.context["repo"]

    where = {}
    if author_id:
        where["author_id"] = author_id
    if status:
        where["status"] = status
    if published is not None:
        where["published"] = published

    return await repo.find("v_post", where=where)
```

### 3. Authentication Pattern

```python
@query
async def my_orders(info, status: str | None = None) -> list[Order]:
    """Get authenticated user's orders."""
    # Extract user from context
    user_context = info.context.get("user")
    if not user_context:
        from graphql import GraphQLError
        raise GraphQLError("Authentication required")

    repo = info.context["repo"]
    where = {"user_id": user_context.user_id}

    if status:
        where["status"] = status

    return await repo.find("v_order", where=where)
```

## Common Errors and Solutions

### Error: "got multiple values for argument"

**Problem:**
```python
@query
async def users(info, limit: int = 10) -> list[User]:
    repo = info.context["repo"]
    # ❌ Wrong: Passing 'limit' twice
    return await repo.find("v_user", limit=limit, limit=20)
```

**Solution:**
```python
@query
async def users(info, limit: int = 10) -> list[User]:
    repo = info.context["repo"]
    # ✅ Correct: Use the parameter value
    return await repo.find("v_user", limit=limit)
```

### Error: Missing `info` parameter

**Problem:**
```python
@query
async def users(limit: int = 10) -> list[User]:
    # ❌ Wrong: No 'info' parameter
    # This will fail when trying to access context
    repo = ???
```

**Solution:**
```python
@query
async def users(info, limit: int = 10) -> list[User]:
    # ✅ Correct: Always include 'info' as first parameter
    repo = info.context["repo"]
    return await repo.find("v_user", limit=limit)
```

### Error: Wrong parameter name

**Problem:**
```python
@query
async def user_by_id(info, user_id: UUID) -> User | None:
    repo = info.context["repo"]
    return await repo.find_one("v_user", id=user_id)

# GraphQL query expects 'userId' but Python has 'user_id'
```

**GraphQL (doesn't work):**
```graphql
{ userById(id: "...") { name } }
# Error: Unknown argument 'id'
```

**Solution - Use exact parameter name:**
```python
@query
async def user_by_id(info, id: UUID) -> User | None:
    # ✅ Correct: Parameter name matches GraphQL argument
    repo = info.context["repo"]
    return await repo.find_one("v_user", id=id)
```

**Or use GraphQL aliases:**
```graphql
{ userById(userId: "...") { name } }
# Works if Python parameter is 'user_id'
```

### Error: Type mismatch

**Problem:**
```python
@query
async def users(info, limit: str) -> list[User]:
    # ❌ Wrong: limit should be int, not str
    repo = info.context["repo"]
    return await repo.find("v_user", limit=int(limit))
```

**Solution:**
```python
@query
async def users(info, limit: int = 10) -> list[User]:
    # ✅ Correct: Use correct type annotation
    repo = info.context["repo"]
    return await repo.find("v_user", limit=limit)
```

## Type Conversion

FraiseQL automatically converts GraphQL types to Python types:

| GraphQL Type | Python Type | Example |
|--------------|-------------|---------|
| `String` | `str` | `"hello"` |
| `Int` | `int` | `42` |
| `Float` | `float` | `3.14` |
| `Boolean` | `bool` | `True` |
| `ID` | `str` or `UUID` | `"123"` or `UUID(...)` |
| `[String]` | `list[str]` | `["a", "b"]` |
| Custom Input | Dataclass | `SearchInput(...)` |

### Custom Type Conversion

```python
from datetime import datetime

@query
async def posts_since(info, since: datetime) -> list[Post]:
    """Get posts since a date."""
    repo = info.context["repo"]
    return await repo.find("v_post", where={"created_at__gte": since})
```

**GraphQL Query:**
```graphql
{ postsSince(since: "2025-01-01T00:00:00Z") { title } }
```

## Advanced: Context Setup

Configure what's available in `info.context`:

```python
from fastapi import Request
from fraiseql import FraiseQL
from fraiseql.fastapi import create_app

async def get_context(request: Request) -> dict:
    """Build GraphQL context from request."""
    context = {"request": request}

    # Add authentication
    token = request.headers.get("Authorization")
    if token:
        user = await verify_token(token)
        context["user"] = user

    # Add tenant isolation
    tenant_id = request.headers.get("X-Tenant-ID")
    context["tenant_id"] = tenant_id

    # Database repository is added automatically
    # context["repo"] is available in all resolvers

    return context

# Create app with custom context
fraiseql_app = FraiseQL(database_url="postgresql://localhost/mydb")
app = create_app(fraiseql_app, context_getter=get_context)
```

Now all resolvers can access:
```python
@query
async def my_data(info) -> MyData:
    repo = info.context["repo"]        # Database
    user = info.context["user"]        # Authenticated user
    tenant_id = info.context["tenant_id"]  # Tenant ID
    request = info.context["request"]  # FastAPI request
```

## Best Practices

### ✅ DO: Always include `info` first

```python
@query
async def users(info, limit: int = 10) -> list[User]:
    pass
```

### ✅ DO: Use type hints for automatic conversion

```python
@query
async def user(info, id: UUID, active: bool = True) -> User | None:
    pass
```

### ✅ DO: Use optional parameters for filters

```python
@query
async def search(
    info,
    name: str | None = None,
    age: int | None = None
) -> list[User]:
    pass
```

### ✅ DO: Use input types for complex arguments

```python
@fraise_input
class SearchInput:
    name: str | None = None
    age_min: int | None = None
    age_max: int | None = None

@query
async def search(info, filters: SearchInput) -> list[User]:
    pass
```

### ❌ DON'T: Forget the `info` parameter

```python
# ❌ WRONG
@query
async def users(limit: int = 10) -> list[User]:
    pass
```

### ❌ DON'T: Use different names in GraphQL and Python

```python
# ❌ CONFUSING (requires GraphQL alias)
@query
async def search(info, search_term: str) -> list[User]:
    pass

# ✅ BETTER (clear parameter name)
@query
async def search(info, query: str) -> list[User]:
    pass
```

### ❌ DON'T: Pass parameters that don't exist in function signature

```python
@query
async def users(info, limit: int = 10) -> list[User]:
    repo = info.context["repo"]
    # ❌ WRONG: 'offset' not in function signature
    return await repo.find("v_user", limit=limit, offset=0)

# ✅ CORRECT: Add offset to signature
@query
async def users(info, limit: int = 10, offset: int = 0) -> list[User]:
    repo = info.context["repo"]
    return await repo.find("v_user", limit=limit, offset=offset)
```

## See Also

- **[Decorators Reference](../api-reference/decorators.md)** - Complete decorator documentation
- **[Repository API](../api-reference/repository.md)** - Database operations
- **[Type System](type-system.md)** - Type definitions and conversion
- **[Troubleshooting](../errors/troubleshooting.md)** - Common errors and solutions

---

**Key Takeaway**: The `info` parameter is automatically injected as the first parameter in all resolvers. All other parameters map directly to GraphQL arguments by name and type.
