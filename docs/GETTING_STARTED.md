# Getting Started with FraiseQL

Welcome to FraiseQL! This guide will walk you through the core concepts and patterns step by step.

## Table of Contents

1. [Core Concepts](#core-concepts)
2. [Installation](#installation)
3. [Your First API](#your-first-api)
4. [Database Integration](#database-integration)
5. [Writing Queries](#writing-queries)
6. [The JSONB Pattern](#the-jsonb-pattern)
7. [Adding Context](#adding-context)
8. [Common Patterns](#common-patterns)
9. [Next Steps](#next-steps)

## Core Concepts

FraiseQL has four fundamental concepts:

### 1. Types are Python Classes
```python
@fraise_type
class User:
    id: UUID
    name: str
    email: str
```

### 2. Queries are Functions
```python
@fraiseql.query
async def get_user(info, id: UUID) -> User:
    # 'info' is ALWAYS the first parameter
    # It contains context like database access
    pass
```

### 3. Data Comes from JSONB Columns
```sql
-- All your views must follow this pattern:
CREATE VIEW user_view AS
SELECT
    id,              -- For filtering
    email,           -- For lookups
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email
    ) as data        -- All object data here (REQUIRED!)
FROM users;
```

### 4. Repository Handles Database Access
```python
# FraiseQL provides a repository that handles connections
db = info.context["db"]  # This is a FraiseQLRepository
user = await db.find_one("user_view", id=user_id)
```

**Important - Repository Modes**:
- **Development Mode**: Returns fully typed Python objects (e.g., `User` instances)
- **Production Mode**: Returns raw dicts for performance
- Set via context or `FRAISEQL_ENV` environment variable

```python
# Development mode example
user = await db.find_one("user_view", id=1)
print(type(user))  # <class 'User'>
print(user.name)   # Direct attribute access

# Production mode example
user = await db.find_one("user_view", id=1)
print(type(user))  # <class 'dict'>
print(user["data"]["name"])  # Dict access
```

## Installation

```bash
pip install fraiseql
```

> ⚠️ **Important**: Version 0.1.0a14+ requires the [JSONB data column pattern](#the-jsonb-pattern). All database views must have a `data` column.

## Your First API

Let's build a simple API without a database first:

```python
# app.py
import fraiseql
from fraiseql import create_fraiseql_app
from datetime import datetime
from uuid import UUID, uuid4

# Step 1: Define a type
@fraiseql.type
class Book:
    id: UUID
    title: str
    author: str
    published: datetime

# Step 2: Create a query
@fraiseql.query
async def books(info) -> list[Book]:
    """Get all books."""
    # For now, return dummy data
    return [
        Book(
            id=uuid4(),
            title="The Great Gatsby",
            author="F. Scott Fitzgerald",
            published=datetime(1925, 4, 10)
        )
    ]

@fraiseql.query
async def book(info, id: UUID) -> Book | None:
    """Get a book by ID."""
    # info is always the first parameter!
    if str(id) == "123e4567-e89b-12d3-a456-426614174000":
        return Book(
            id=id,
            title="1984",
            author="George Orwell",
            published=datetime(1949, 6, 8)
        )
    return None

# Step 3: Create the app
if __name__ == "__main__":
    import uvicorn

    app = create_fraiseql_app(
        types=[Book],  # Register your types
        production=False  # Enable GraphQL Playground
    )

    print("🚀 GraphQL Playground: http://localhost:8000/graphql")
    uvicorn.run(app, port=8000)
```

Run it:
```bash
python app.py
```

Try this query in the Playground:
```graphql
{
  books {
    id
    title
    author
  }
}
```

## Database Integration

Now let's connect to a real PostgreSQL database:

### 1. Create Your Database View

```sql
-- CRITICAL: Your view MUST follow this pattern!
CREATE VIEW book_view AS
SELECT
    id,                      -- For filtering
    author,                  -- For author queries
    published,               -- For date filtering
    jsonb_build_object(      -- All data in 'data' column
        'id', id,
        'title', title,
        'author', author,
        'published', published
    ) as data
FROM books;
```

### 2. Update Your Queries

```python
@fraiseql.query
async def books(info, author: str | None = None) -> list[Book]:
    """Get all books, optionally filtered by author."""
    # Access the database through info.context
    db = info.context["db"]  # This is a FraiseQLRepository

    if author:
        # Use the filtering column, not the JSONB data
        return await db.find("book_view", author=author)
    else:
        return await db.find("book_view")

@fraiseql.query
async def book(info, id: UUID) -> Book | None:
    """Get a book by ID."""
    db = info.context["db"]
    return await db.find_one("book_view", id=id)
```

### 3. Create App with Database

```python
app = create_fraiseql_app(
    database_url="postgresql://user:pass@localhost/mydb",
    types=[Book],
    production=False
)
```

## Writing Queries

### The @fraiseql.query Pattern (Recommended)

```python
@fraiseql.query
async def my_query(info, arg1: str, arg2: int = 10) -> ReturnType:
    """Query description."""
    # info is ALWAYS first parameter
    # Access context through info.context
    db = info.context["db"]
    user = info.context.get("user")  # If auth is enabled

    # Your logic here
    return result
```

**Key Rules:**
1. First parameter is ALWAYS `info`
2. Use `@fraiseql.query` decorator
3. Return type annotation is required
4. Async is recommended but not required

### Common Mistakes to Avoid

```python
# ❌ WRONG: Don't use resolve_ prefix
class Query:
    async def resolve_users(self, info):
        pass

# ✅ CORRECT: Use @fraiseql.query
@fraiseql.query
async def users(info) -> list[User]:
    pass

# ❌ WRONG: Don't create two-tier resolvers
@fraiseql.query
async def users(info):
    return await some_other_resolver(info)

# ✅ CORRECT: Implement directly
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view")
```

## The JSONB Pattern

This is a **CRITICAL** concept in FraiseQL v0.1.0a14+:

### Why This Pattern?

1. **Performance**: Single column retrieval
2. **Flexibility**: Easy to add fields
3. **Type Safety**: Automatic instantiation
4. **Consistency**: One pattern for all data

### Database View Requirements

```sql
-- Every view MUST have this structure:
CREATE VIEW entity_view AS
SELECT
    -- Filtering columns (used in WHERE clauses)
    id,
    tenant_id,
    status,
    created_at,

    -- Data column (used for object instantiation)
    jsonb_build_object(
        'id', id,
        'field1', field1,
        'field2', field2,
        -- Nested objects
        'related_object', jsonb_build_object(
            'id', r.id,
            'name', r.name
        )
    ) as data  -- THIS COLUMN IS REQUIRED!
FROM entity e
LEFT JOIN related r ON r.entity_id = e.id;
```

### How It Works

1. **Query** uses filtering columns:
   ```python
   await db.find("entity_view", status='active', tenant_id=tenant_id)
   ```

2. **FraiseQL** instantiates from `data` column:
   ```python
   # In development mode: Returns typed Entity objects
   # In production mode: Returns raw dicts for performance
   ```

## Adding Context

### Default Context

FraiseQL provides these by default:
- `db`: FraiseQLRepository instance
- `user`: Current user (if auth enabled)
- `authenticated`: Boolean flag

### Custom Context

Add your own values:

```python
async def get_context(request: Request) -> dict[str, Any]:
    """Build custom context."""
    pool = request.app.state.db_pool

    # Create repository with custom context
    repo = FraiseQLRepository(pool, context={
        "tenant_id": request.headers.get("tenant-id"),
        "mode": "development"  # or "production"
    })

    return {
        "db": repo,
        "request": request,
        "tenant_id": request.headers.get("tenant-id"),
        "custom_value": "anything"
    }

# Use in app
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[Book],
    context_getter=get_context
)
```

## Common Patterns

### Multi-Tenant Queries

```python
@fraiseql.query
async def tenant_books(info) -> list[Book]:
    """Get books for current tenant."""
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]

    return await db.find("book_view", tenant_id=tenant_id)
```

### Authenticated Queries

```python
from fraiseql.auth import requires_auth

@fraiseql.query
@requires_auth
async def my_books(info) -> list[Book]:
    """Get current user's books."""
    db = info.context["db"]
    user = info.context["user"]  # Guaranteed to exist

    return await db.find("book_view", user_id=user.id)
```

### Pagination

```python
@fraiseql.query
async def paginated_books(
    info,
    page: int = 1,
    per_page: int = 20
) -> dict[str, Any]:
    """Get paginated books."""
    db = info.context["db"]
    offset = (page - 1) * per_page

    books = await db.find("book_view", limit=per_page, offset=offset)
    total = await db.count("book_view")  # Implement count method

    return {
        "items": books,
        "total": total,
        "page": page,
        "per_page": per_page,
        "pages": (total + per_page - 1) // per_page
    }
```

## Next Steps

1. Read the [Architecture Guide](ARCHITECTURE.md) to understand design decisions
2. Check the [API Reference](API_REFERENCE.md) for all decorators and methods
3. See [Common Patterns](COMMON_PATTERNS.md) for real-world examples
4. Review [Troubleshooting](TROUBLESHOOTING.md) for common issues

## Quick Reference

```python
# 1. Type definition
@fraise_type
class MyType:
    field: type

# 2. Query definition
@fraiseql.query
async def query_name(info, args) -> ReturnType:
    db = info.context["db"]
    return await db.find("view_name")

# 3. Database view
CREATE VIEW view_name AS
SELECT id, filter_columns, jsonb_build_object(...) as data FROM table;

# 4. App creation
app = create_fraiseql_app(
    database_url="postgresql://...",
    types=[MyType],
    production=False
)
```

Remember: **info is always first, data is always in JSONB!**
