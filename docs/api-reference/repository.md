# FraiseQLRepository API Reference

## Overview

The `FraiseQLRepository` is the primary interface for database operations in FraiseQL. It provides a consistent API for querying views and calling PostgreSQL functions, following the CQRS pattern where views handle queries and functions handle mutations.

## Table of Contents

1. [Getting the Repository](#getting-the-repository)
2. [Query Methods](#query-methods)
   - [find()](#find)
   - [find_one()](#find_one)
   - [count()](#count)
3. [Mutation Methods](#mutation-methods)
   - [call_function()](#call_function)
4. [Advanced Methods](#advanced-methods)
   - [run()](#run)
   - [fetch_one()](#fetch_one)
   - [fetch_many()](#fetch_many)
5. [Transaction Support](#transaction-support)
6. [Context and Modes](#context-and-modes)
7. [Common Patterns](#common-patterns)

---

## Getting the Repository

The repository is always available in the GraphQL context:

```python
@fraiseql.query
async def my_query(info) -> Result:
    db = info.context["db"]  # FraiseQLRepository instance
    # Use db for all database operations
```

### Never Do This

```python
# ❌ WRONG: Don't create your own repository
@fraiseql.query
async def bad_query(info) -> Result:
    # Don't do this!
    db = FraiseQLRepository(connection)
    
# ❌ WRONG: Don't use raw connections
@fraiseql.query
async def bad_query(info) -> Result:
    # Don't do this!
    conn = await asyncpg.connect(...)
```

---

## Query Methods

### find()

Query multiple records from a database view.

#### Signature

```python
async def find(
    self,
    view_name: str,
    **filters: Any
) -> list[T]
```

#### Parameters

- `view_name`: Name of the database view to query
- `**filters`: Keyword arguments for WHERE conditions

#### Returns

- List of objects instantiated from the view's `data` column
- Empty list if no matches found

#### Examples

```python
# Simple query - all records
@fraiseql.query
async def all_users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view")

# With filters
@fraiseql.query
async def active_users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view", is_active=True)

# Multiple filters (AND condition)
@fraiseql.query
async def users_by_role_and_status(info, role: str) -> list[User]:
    db = info.context["db"]
    return await db.find(
        "user_view",
        role=role,
        is_active=True,
        verified=True
    )

# With NULL checks
@fraiseql.query
async def users_without_profile(info) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view", profile_id=None)
```

#### Important Notes

1. **View must have a `data` column** containing JSONB
2. **Filters use the view's columns**, not the JSONB data
3. **All filters are combined with AND**
4. **None values translate to IS NULL**

#### View Example

```sql
CREATE VIEW user_view AS
SELECT
    id,              -- Available for filtering
    email,           -- Available for filtering  
    role,            -- Available for filtering
    is_active,       -- Available for filtering
    jsonb_build_object(
        'id', id,
        'email', email,
        'name', name,
        'role', role,
        'is_active', is_active
    ) as data        -- Required: Object data
FROM users;
```

---

### find_one()

Query a single record from a database view.

#### Signature

```python
async def find_one(
    self,
    view_name: str,
    **filters: Any
) -> Optional[T]
```

#### Parameters

- `view_name`: Name of the database view to query
- `**filters`: Keyword arguments for WHERE conditions

#### Returns

- Single object instance if found
- `None` if no match

#### Examples

```python
# Find by ID
@fraiseql.query
async def user(info, id: UUID) -> Optional[User]:
    db = info.context["db"]
    return await db.find_one("user_view", id=id)

# Find by unique field
@fraiseql.query
async def user_by_email(info, email: str) -> Optional[User]:
    db = info.context["db"]
    return await db.find_one("user_view", email=email)

# Multiple conditions
@fraiseql.query
async def active_user_by_email(info, email: str) -> Optional[User]:
    db = info.context["db"]
    return await db.find_one(
        "user_view",
        email=email,
        is_active=True
    )
```

#### Common Pattern: 404 Handling

```python
from fraiseql import GraphQLError

@fraiseql.query
async def user_or_error(info, id: UUID) -> User:
    db = info.context["db"]
    user = await db.find_one("user_view", id=id)
    if not user:
        raise GraphQLError(f"User {id} not found")
    return user
```

---

### count()

Count records in a view matching filters.

#### Signature

```python
async def count(
    self,
    view_name: str,
    **filters: Any
) -> int
```

#### Examples

```python
# Total count
@fraiseql.query
async def total_users(info) -> int:
    db = info.context["db"]
    return await db.count("user_view")

# Filtered count
@fraiseql.query
async def active_user_count(info) -> int:
    db = info.context["db"]
    return await db.count("user_view", is_active=True)

# For pagination
@fraiseql.query
async def paginated_users(info, page: int = 1, per_page: int = 20) -> PaginatedUsers:
    db = info.context["db"]
    
    offset = (page - 1) * per_page
    users = await db.find("user_view", limit=per_page, offset=offset)
    total = await db.count("user_view")
    
    return PaginatedUsers(
        items=users,
        total=total,
        page=page,
        pages=(total + per_page - 1) // per_page
    )
```

---

## Mutation Methods

### call_function()

Call a PostgreSQL function (stored procedure) for mutations.

#### Signature

```python
async def call_function(
    self,
    function_name: str,
    **params: Any
) -> dict[str, Any]
```

#### Parameters

- `function_name`: Name of the PostgreSQL function
- `**params`: Function parameters as keyword arguments

#### Returns

- Dictionary with the function's result

#### Examples

```python
# Create mutation
@fraiseql.mutation
async def create_user(info, input: CreateUserInput) -> User:
    db = info.context["db"]
    
    result = await db.call_function(
        "create_user",
        email=input.email,
        name=input.name,
        password_hash=hash_password(input.password)
    )
    
    return User(**result)

# Update mutation
@fraiseql.mutation
async def update_user(info, id: UUID, input: UpdateUserInput) -> User:
    db = info.context["db"]
    
    result = await db.call_function(
        "update_user",
        user_id=id,
        updates=input.__dict__  # Pass as JSONB
    )
    
    return User(**result)

# Delete mutation
@fraiseql.mutation
async def delete_user(info, id: UUID) -> bool:
    db = info.context["db"]
    
    result = await db.call_function(
        "delete_user",
        user_id=id
    )
    
    return result["success"]
```

#### PostgreSQL Function Example

```sql
CREATE OR REPLACE FUNCTION create_user(
    email text,
    name text,
    password_hash text
) RETURNS jsonb AS $$
DECLARE
    new_user_id uuid;
    result jsonb;
BEGIN
    INSERT INTO users (email, name, password_hash)
    VALUES (email, name, password_hash)
    RETURNING id INTO new_user_id;
    
    SELECT data INTO result
    FROM user_view
    WHERE id = new_user_id;
    
    RETURN result;
END;
$$ LANGUAGE plpgsql;
```

---

## Advanced Methods

### run()

Execute raw SQL queries with full control.

#### Signature

```python
async def run(self, query: DatabaseQuery) -> Any
```

#### Examples

```python
from fraiseql.db import DatabaseQuery
from psycopg.sql import SQL

# Custom query
@fraiseql.query
async def search_posts(info, search_term: str) -> list[Post]:
    db = info.context["db"]
    
    query = DatabaseQuery(
        statement=SQL("""
            SELECT data
            FROM post_view
            WHERE to_tsvector('english', title || ' ' || content) 
                @@ plainto_tsquery('english', %(search)s)
            ORDER BY ts_rank(
                to_tsvector('english', title || ' ' || content),
                plainto_tsquery('english', %(search)s)
            ) DESC
        """),
        params={"search": search_term},
        fetch_result=True
    )
    
    results = await db.run(query)
    return [Post(**row["data"]) for row in results]
```

### fetch_one()

Execute raw SQL returning a single row.

```python
@fraiseql.query
async def user_stats(info, user_id: UUID) -> dict:
    db = info.context["db"]
    
    result = await db.fetch_one("""
        SELECT 
            COUNT(DISTINCT p.id) as post_count,
            COUNT(DISTINCT c.id) as comment_count,
            MAX(p.created_at) as last_post_date
        FROM users u
        LEFT JOIN posts p ON p.author_id = u.id
        LEFT JOIN comments c ON c.author_id = u.id
        WHERE u.id = %s
        GROUP BY u.id
    """, (user_id,))
    
    return dict(result) if result else {}
```

### fetch_many()

Execute raw SQL returning multiple rows.

```python
@fraiseql.query
async def user_activity_feed(info, user_id: UUID, limit: int = 50) -> list[Activity]:
    db = info.context["db"]
    
    results = await db.fetch_many("""
        (
            SELECT 'post' as type, id, title as content, created_at
            FROM posts WHERE author_id = %s
        ) UNION ALL (
            SELECT 'comment' as type, id, content, created_at  
            FROM comments WHERE author_id = %s
        )
        ORDER BY created_at DESC
        LIMIT %s
    """, (user_id, user_id, limit))
    
    return [Activity(**dict(row)) for row in results]
```

---

## Transaction Support

Use transactions for atomic operations:

```python
@fraiseql.mutation
async def transfer_credits(
    info,
    from_user_id: UUID,
    to_user_id: UUID,
    amount: int
) -> TransferResult:
    db = info.context["db"]
    
    async with db._pool.connection() as conn:
        async with conn.transaction():
            # Deduct from sender
            await conn.execute("""
                UPDATE users 
                SET credits = credits - %s 
                WHERE id = %s AND credits >= %s
            """, (amount, from_user_id, amount))
            
            # Add to receiver
            await conn.execute("""
                UPDATE users 
                SET credits = credits + %s 
                WHERE id = %s
            """, (amount, to_user_id))
            
            # Create transfer record
            await conn.execute("""
                INSERT INTO transfers (from_user_id, to_user_id, amount)
                VALUES (%s, %s, %s)
            """, (from_user_id, to_user_id, amount))
    
    return TransferResult(success=True, amount=amount)
```

---

## Context and Modes

### Repository Context

The repository can store context values:

```python
# Setting context during creation
async def get_context(request):
    repo = FraiseQLRepository(
        pool=db_pool,
        context={
            "tenant_id": request.headers.get("x-tenant-id"),
            "request_id": str(uuid4()),
            "user_agent": request.headers.get("user-agent")
        }
    )
    return {"db": repo}
```

### Development vs Production Mode

```python
# The repository behaves differently based on mode
repo = FraiseQLRepository(pool, mode="development")  # Full object instantiation
repo = FraiseQLRepository(pool, mode="production")   # Optimized queries

# Mode affects:
# - Object instantiation (partial in dev, full in prod)
# - Error messages (detailed in dev, safe in prod)
# - Query logging (verbose in dev, minimal in prod)
```

---

## Common Patterns

### Multi-Tenant Queries

```python
@fraiseql.query
async def tenant_users(info) -> list[User]:
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]
    
    # Always filter by tenant
    return await db.find("user_view", tenant_id=tenant_id)

@fraiseql.query
async def tenant_user(info, id: UUID) -> Optional[User]:
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]
    
    # Include tenant check in single queries too!
    return await db.find_one(
        "user_view",
        id=id,
        tenant_id=tenant_id
    )
```

### Pagination Pattern

```python
@fraiseql.type
class PaginatedUsers:
    items: list[User]
    total: int
    page: int
    per_page: int
    has_next: bool
    has_prev: bool

@fraiseql.query
async def users_paginated(
    info,
    page: int = 1,
    per_page: int = 20,
    filter: Optional[UserFilter] = None
) -> PaginatedUsers:
    db = info.context["db"]
    
    offset = (page - 1) * per_page
    
    # Build filters
    filters = {}
    if filter:
        if filter.role:
            filters["role"] = filter.role
        if filter.is_active is not None:
            filters["is_active"] = filter.is_active
    
    # Get items and count
    items = await db.find(
        "user_view",
        limit=per_page,
        offset=offset,
        **filters
    )
    total = await db.count("user_view", **filters)
    
    return PaginatedUsers(
        items=items,
        total=total,
        page=page,
        per_page=per_page,
        has_next=page * per_page < total,
        has_prev=page > 1
    )
```

### Batch Loading Pattern

```python
@fraiseql.query
async def users_with_stats(info) -> list[UserWithStats]:
    db = info.context["db"]
    
    # Get users
    users = await db.find("user_view", is_active=True)
    user_ids = [u.id for u in users]
    
    # Batch load stats
    stats = await db.fetch_many("""
        SELECT 
            author_id as user_id,
            COUNT(*) as post_count,
            MAX(created_at) as last_post_date
        FROM posts
        WHERE author_id = ANY(%s)
        GROUP BY author_id
    """, (user_ids,))
    
    # Combine data
    stats_map = {s["user_id"]: s for s in stats}
    
    return [
        UserWithStats(
            **user.__dict__,
            post_count=stats_map.get(user.id, {}).get("post_count", 0),
            last_post_date=stats_map.get(user.id, {}).get("last_post_date")
        )
        for user in users
    ]
```

### Error Handling Pattern

```python
from psycopg import errors

@fraiseql.mutation
async def create_user_safe(info, input: CreateUserInput) -> CreateUserResult:
    db = info.context["db"]
    
    try:
        result = await db.call_function(
            "create_user",
            email=input.email,
            name=input.name
        )
        return CreateUserSuccess(user=User(**result))
        
    except errors.UniqueViolation as e:
        if "users_email_key" in str(e):
            return CreateUserError(
                message="Email already exists",
                code="DUPLICATE_EMAIL",
                field="email"
            )
        raise
        
    except errors.CheckViolation as e:
        return CreateUserError(
            message="Invalid data provided",
            code="VALIDATION_ERROR"
        )
```

## Best Practices

1. **Always use the repository from context** - Never create your own instance
2. **Use views for queries** - All queries should go through database views
3. **Use functions for mutations** - Mutations should call PostgreSQL functions
4. **Include filter columns in views** - Make sure views expose columns for filtering
5. **Handle None values** - None in filters translates to IS NULL
6. **Use transactions for complex mutations** - Ensure atomicity
7. **Consider pagination early** - Add limit/offset support to views
8. **Test both dev and production modes** - Behavior can differ

## Next Steps

- Learn about [Context Management](./context.md)
- Explore [Query Patterns](../patterns/queries.md)
- Understand [Database Views](../patterns/database.md)