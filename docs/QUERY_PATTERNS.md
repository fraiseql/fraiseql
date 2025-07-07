# FraiseQL Query Patterns

This guide explains how to write queries in FraiseQL, which differs from traditional GraphQL approaches.

## Table of Contents

1. [The FraiseQL Way](#the-fraiseql-way)
2. [Query Functions](#query-functions)
3. [Common Mistakes](#common-mistakes)
4. [The Info Parameter](#the-info-parameter)
5. [Database Access](#database-access)
6. [Return Types](#return-types)
7. [Arguments and Filters](#arguments-and-filters)
8. [Error Handling](#error-handling)
9. [Advanced Patterns](#advanced-patterns)
10. [Migration from Traditional GraphQL](#migration-from-traditional-graphql)

## The FraiseQL Way

FraiseQL uses a **function-based approach** instead of resolver classes. This is simpler, more direct, and easier to understand.

### Traditional GraphQL (NOT FraiseQL)
```python
# ❌ THIS IS NOT HOW FRAISEQL WORKS
class Query:
    async def resolve_users(self, info):
        # Complex resolver logic
        pass

    async def resolve_user(self, info, id: str):
        # More resolver logic
        pass
```

### FraiseQL Pattern (The Right Way)
```python
# ✅ THIS IS THE FRAISEQL WAY
@fraiseql.query
async def users(info) -> list[User]:
    """Get all users."""
    db = info.context["db"]
    return await db.find("user_view")

@fraiseql.query
async def user(info, id: UUID) -> User | None:
    """Get a single user by ID."""
    db = info.context["db"]
    return await db.find_one("user_view", id=id)
```

## Query Functions

### Basic Structure

Every query in FraiseQL follows this pattern:

```python
@fraiseql.query
async def query_name(info, arg1: Type1, arg2: Type2 = default) -> ReturnType:
    """Query description for GraphQL schema."""
    # Implementation
    pass
```

**Key Rules:**
1. **Always use `@fraiseql.query` decorator**
2. **First parameter is ALWAYS `info`**
3. **Return type annotation is REQUIRED**
4. **Async is recommended but not required**
5. **Docstring becomes GraphQL description**

### Examples

#### Simple Query
```python
@fraiseql.query
async def current_time(info) -> datetime:
    """Get the current server time."""
    return datetime.now()
```

#### Query with Arguments
```python
@fraiseql.query
async def books_by_author(info, author: str) -> list[Book]:
    """Get all books by a specific author."""
    db = info.context["db"]
    return await db.find("book_view", author=author)
```

#### Query with Optional Arguments
```python
@fraiseql.query
async def search_books(
    info,
    title: str | None = None,
    author: str | None = None,
    published_after: datetime | None = None
) -> list[Book]:
    """Search books with optional filters."""
    db = info.context["db"]

    filters = {}
    if title:
        filters["title"] = title
    if author:
        filters["author"] = author
    if published_after:
        filters["published_after"] = published_after

    return await db.find("book_view", **filters)
```

## Common Mistakes

### 1. Using resolve_ Prefix

```python
# ❌ WRONG: Don't use resolve_ prefix
class Query:
    async def resolve_users(self, info):
        pass

# ✅ CORRECT: Use @fraiseql.query
@fraiseql.query
async def users(info) -> list[User]:
    pass
```

### 2. Wrong Parameter Order

```python
# ❌ WRONG: info must be first
@fraiseql.query
async def get_user(id: UUID, info) -> User:
    pass

# ✅ CORRECT: info is always first
@fraiseql.query
async def get_user(info, id: UUID) -> User:
    pass
```

### 3. Missing Return Type

```python
# ❌ WRONG: No return type annotation
@fraiseql.query
async def users(info):
    return await db.find("user_view")

# ✅ CORRECT: Return type is required
@fraiseql.query
async def users(info) -> list[User]:
    return await db.find("user_view")
```

### 4. Two-Tier Resolution

```python
# ❌ WRONG: Don't create wrapper functions
@fraiseql.query
async def users(info):
    return await UserResolver.get_all_users(info)

# ✅ CORRECT: Implement directly
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view")
```

### 5. Defining But Not Using Where Inputs

```python
# ❌ WRONG: Defining where parameter but ignoring it
@fraiseql.query
async def machines(info, where: MachineWhereInput | None = None) -> list[Machine]:
    db = info.context["db"]
    # where parameter is completely ignored!
    return await db.find("machine_view", tenant_id=tenant_id)

# ✅ CORRECT: Actually use the where input
@fraiseql.query
async def machines(info, where: MachineWhereInput | None = None) -> list[Machine]:
    db = info.context["db"]
    filters = _build_machine_filters(where, tenant_id)
    return await db.find("machine_view", **filters)

def _build_machine_filters(where: MachineWhereInput | dict | None, tenant_id: str) -> dict[str, Any]:
    filters = {"tenant_id": tenant_id}

    if not where:
        return filters

    # Handle both dict and object input
    get_field = (lambda f: where.get(f)) if isinstance(where, dict) else (lambda f: getattr(where, f, None))

    if get_field('status'):
        filters['status'] = get_field('status')
    if get_field('is_active') is not None:
        filters['is_active'] = get_field('is_active')

    return filters
```

### 6. Not Handling Dict Input Types

```python
# ❌ WRONG: Assuming input is always a typed object
@fraiseql.query
async def users(info, where: UserWhereInput | None = None) -> list[User]:
    if where:
        # This fails if where is a dict!
        email = where.email  # AttributeError: 'dict' has no attribute 'email'

# ✅ CORRECT: Handle both dict and object inputs
@fraiseql.query
async def users(info, where: UserWhereInput | None = None) -> list[User]:
    if where:
        # Safe access that works for both types
        email = where.get('email') if isinstance(where, dict) else where.email
```

### 7. Creating Query Classes

```python
# ❌ WRONG: Using Query class
@fraiseql.type
class Query:
    async def machines(self, info, limit: int = 20) -> list[Machine]:
        pass

# Then trying to register it:
app = create_fraiseql_app(queries=[Query])  # This causes "no fields" error!

# ✅ CORRECT: Use function decorators
@fraiseql.query
async def machines(info, limit: int = 20) -> list[Machine]:
    db = info.context["db"]
    return await db.find("machine_view", limit=limit)

# No need to pass queries parameter:
app = create_fraiseql_app(types=[Machine])  # Queries auto-discovered!
```

## The Info Parameter

The `info` parameter is your gateway to everything you need:

```python
@fraiseql.query
async def my_query(info) -> Any:
    # Access database
    db = info.context["db"]  # FraiseQLRepository

    # Access authenticated user (if auth enabled)
    user = info.context.get("user")  # UserContext or None

    # Check if authenticated
    is_authenticated = info.context["authenticated"]  # bool

    # Access request
    request = info.context["request"]  # FastAPI Request

    # Access custom context values
    tenant_id = info.context.get("tenant_id")  # Your custom values
```

### What's in info.context?

| Key | Type | Description |
|-----|------|-------------|
| `db` | `FraiseQLRepository` | Database access (always present) |
| `user` | `UserContext \| None` | Authenticated user info |
| `authenticated` | `bool` | Whether user is authenticated |
| `request` | `Request` | FastAPI request object |
| `loader_registry` | `LoaderRegistry` | DataLoader registry |
| *custom* | `Any` | Your custom context values |

## Database Access

### The Repository Pattern

FraiseQL provides a `FraiseQLRepository` that handles all database operations:

```python
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]  # This is a FraiseQLRepository

    # Find multiple records
    all_users = await db.find("user_view")

    # Find with filters
    active_users = await db.find("user_view", status="active")

    # Find with pagination
    page_1 = await db.find("user_view", limit=10, offset=0)

    # Find single record
    user = await db.find_one("user_view", id=user_id)
```

### Database View Requirements

Your views MUST follow the JSONB pattern:

```sql
CREATE VIEW user_view AS
SELECT
    -- Filtering columns (used in WHERE clauses)
    id,
    email,
    status,
    tenant_id,

    -- Data column (used for object instantiation)
    jsonb_build_object(
        'id', id,
        'email', email,
        'name', name,
        'status', status,
        'created_at', created_at
    ) as data  -- THIS COLUMN IS REQUIRED!
FROM users;
```

## Return Types

### Supported Return Types

```python
# Single object (nullable)
@fraiseql.query
async def user(info, id: UUID) -> User | None:
    pass

# List of objects
@fraiseql.query
async def users(info) -> list[User]:
    pass

# Scalar values
@fraiseql.query
async def user_count(info) -> int:
    pass

# Custom types
@fraise_type
class SearchResult:
    users: list[User]
    total: int

@fraiseql.query
async def search(info, query: str) -> SearchResult:
    pass
```

### Development vs Production Mode

```python
# In development mode:
user = await db.find_one("user_view", id=1)
print(type(user))  # <class 'User'>
print(user.name)   # Direct attribute access

# In production mode:
user = await db.find_one("user_view", id=1)
print(type(user))  # <class 'dict'>
print(user["data"]["name"])  # Dict access
```

## Arguments and Filters

### Basic Arguments

```python
@fraiseql.query
async def user(info, id: UUID) -> User | None:
    """Get user by ID."""
    db = info.context["db"]
    return await db.find_one("user_view", id=id)
```

### Optional Arguments with Defaults

```python
@fraiseql.query
async def users(
    info,
    limit: int = 20,
    offset: int = 0,
    sort_by: str = "created_at"
) -> list[User]:
    """Get paginated users."""
    db = info.context["db"]
    return await db.find("user_view",
        limit=limit,
        offset=offset,
        order_by=sort_by
    )
```

### Complex Filters

```python
@fraise_input
class UserFilter:
    role: str | None = None
    status: str | None = None
    created_after: datetime | None = None
    created_before: datetime | None = None

@fraiseql.query
async def filtered_users(info, filter: UserFilter | None = None) -> list[User]:
    """Get users with complex filtering."""
    db = info.context["db"]

    kwargs = {}
    if filter:
        if filter.role:
            kwargs["role"] = filter.role
        if filter.status:
            kwargs["status"] = filter.status
        # Add date filtering logic

    return await db.find("user_view", **kwargs)
```

## Error Handling

### Using GraphQL Errors

```python
from graphql import GraphQLError

@fraiseql.query
async def user(info, id: UUID) -> User:
    """Get user by ID (required to exist)."""
    db = info.context["db"]
    user = await db.find_one("user_view", id=id)

    if not user:
        raise GraphQLError(f"User {id} not found")

    return user
```

### Handling Database Errors

```python
@fraiseql.query
async def users(info) -> list[User]:
    """Get all users with error handling."""
    try:
        db = info.context["db"]
        return await db.find("user_view")
    except Exception as e:
        logger.error(f"Failed to fetch users: {e}")
        raise GraphQLError("Failed to fetch users")
```

### Custom Error Types

```python
@fraise_type
class UserResult:
    user: User | None = None
    error: str | None = None

@fraiseql.query
async def safe_user(info, id: UUID) -> UserResult:
    """Get user with safe error handling."""
    db = info.context["db"]

    try:
        user = await db.find_one("user_view", id=id)
        if not user:
            return UserResult(error=f"User {id} not found")
        return UserResult(user=user)
    except Exception as e:
        return UserResult(error="Internal error")
```

## Advanced Patterns

### Multi-Tenant Queries

```python
@fraiseql.query
async def tenant_users(info) -> list[User]:
    """Get users for current tenant."""
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]  # From custom context

    if not tenant_id:
        raise GraphQLError("Tenant ID required")

    return await db.find("user_view", tenant_id=tenant_id)
```

### Authenticated Queries

```python
from fraiseql.auth import requires_auth

@fraiseql.query
@requires_auth
async def my_profile(info) -> User:
    """Get current user's profile."""
    db = info.context["db"]
    user = info.context["user"]  # Guaranteed to exist with @requires_auth

    profile = await db.find_one("user_view", id=user.user_id)
    if not profile:
        raise GraphQLError("Profile not found")

    return profile
```

### Nested Data Loading

```python
@fraise_type
class UserWithPosts:
    id: UUID
    name: str
    email: str
    posts: list[Post]

@fraiseql.query
async def user_with_posts(info, id: UUID) -> UserWithPosts | None:
    """Get user with all their posts."""
    db = info.context["db"]

    # View handles the join and nesting
    return await db.find_one("user_with_posts_view", id=id)
```

The view would look like:
```sql
CREATE VIEW user_with_posts_view AS
SELECT
    u.id,
    jsonb_build_object(
        'id', u.id,
        'name', u.name,
        'email', u.email,
        'posts', COALESCE(
            jsonb_agg(
                jsonb_build_object(
                    'id', p.id,
                    'title', p.title,
                    'content', p.content
                ) ORDER BY p.created_at DESC
            ) FILTER (WHERE p.id IS NOT NULL),
            '[]'::jsonb
        )
    ) as data
FROM users u
LEFT JOIN posts p ON p.author_id = u.id
GROUP BY u.id;
```

## Migration from Traditional GraphQL

If you're coming from traditional GraphQL, here's how to migrate:

### From Resolver Classes

```python
# OLD: Traditional GraphQL
class Query:
    async def resolve_users(self, info):
        return await fetch_users()

    async def resolve_user(self, info, id: str):
        return await fetch_user(id)

# NEW: FraiseQL
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view")

@fraiseql.query
async def user(info, id: UUID) -> User | None:
    db = info.context["db"]
    return await db.find_one("user_view", id=id)
```

### From Field Resolvers

```python
# OLD: Field resolver
class UserType:
    async def resolve_posts(self, info):
        return await fetch_posts_for_user(self.id)

# NEW: Use database views with JSONB
CREATE VIEW user_with_posts AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'posts', (
            SELECT jsonb_agg(...)
            FROM posts WHERE author_id = u.id
        )
    ) as data
FROM users u;
```

### From DataLoaders

```python
# OLD: DataLoader pattern
class UserLoader(DataLoader):
    async def batch_load_fn(self, user_ids):
        # Batch loading logic

# NEW: Let the database handle it
@fraiseql.query
async def users_by_ids(info, ids: list[UUID]) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view", id=ids)  # IN clause
```

## Best Practices

1. **Always specify return types** - Required for schema generation
2. **Use descriptive docstrings** - They become GraphQL descriptions
3. **Handle errors gracefully** - Use GraphQLError for client-friendly messages
4. **Let the database do the work** - Use views for complex queries
5. **Keep queries simple** - If it's complex, it probably belongs in a view
6. **Use filters in WHERE** - Not in Python code
7. **Test both modes** - Development and production behave differently

## Summary

FraiseQL queries are just Python functions with:
- `@fraiseql.query` decorator
- `info` as the first parameter
- Type annotations for arguments and return values
- Direct database access through `info.context["db"]`

No resolver classes, no field resolvers, no complex patterns. Just functions that return data.
