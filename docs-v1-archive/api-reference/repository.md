# Repository API Reference

**Complete reference for FraiseQL's `CQRSRepository` - the data access layer for database operations.**

## Overview

The `CQRSRepository` class implements the Repository pattern, providing a clean abstraction over PostgreSQL operations. It supports CQRS (Command Query Responsibility Segregation) with optimized methods for both reads and writes.

### Import

```python
from fraiseql.cqrs import CQRSRepository
```

### Initialization

```python
# From FastAPI context
repo = info.context["repo"]

# Manual initialization (testing, scripts)
import psycopg
from fraiseql.cqrs import CQRSRepository

conn = await psycopg.AsyncConnection.connect("postgresql://...")
repo = CQRSRepository(conn)
```

---

## Query Methods (CQRS Read Side)

### `find()`

**Signature:**
```python
async def find(
    view_name: str,
    where: dict | None = None,
    order_by: list[dict] | None = None,
    limit: int | None = None,
    offset: int | None = None,
    **kwargs
) -> list[dict]
```

**Description:** Query multiple records from a PostgreSQL view.

**Parameters:**
- `view_name` (str): Name of the view to query (e.g., `"v_user"`, `"tv_user_stats"`)
- `where` (dict | None): Filter conditions as key-value pairs
- `order_by` (list[dict] | None): Sorting specification
- `limit` (int | None): Maximum number of records to return
- `offset` (int | None): Number of records to skip (pagination)
- `**kwargs`: Additional filter conditions (merged with `where`)

**Returns:** List of dictionaries representing rows

**Examples:**

```python
# Basic query
users = await repo.find("v_user")

# With filters
active_users = await repo.find(
    "v_user",
    where={"active": True, "role": "admin"}
)

# With ordering
users = await repo.find(
    "v_user",
    order_by=[{"created_at": "desc"}],
    limit=10
)

# Pagination
page_2_users = await repo.find(
    "v_user",
    limit=20,
    offset=20  # Skip first 20
)

# Kwargs style (alternative to where dict)
admins = await repo.find("v_user", role="admin", active=True)

# Complex filters
users = await repo.find(
    "v_user",
    where={
        "age__gte": 18,           # Greater than or equal
        "name__icontains": "john", # Case-insensitive contains
        "created_at__lt": "2024-01-01"
    }
)
```

**Filter Operators:**
- `field`: Exact match
- `field__eq`: Equals
- `field__ne`: Not equals
- `field__gt`: Greater than
- `field__gte`: Greater than or equal
- `field__lt`: Less than
- `field__lte`: Less than or equal
- `field__in`: In list
- `field__contains`: Contains substring (case-sensitive)
- `field__icontains`: Contains substring (case-insensitive)
- `field__startswith`: Starts with
- `field__endswith`: Ends with

---

### `find_one()`

**Signature:**
```python
async def find_one(
    view_name: str,
    where: dict | None = None,
    **kwargs
) -> dict | None
```

**Description:** Query a single record from a PostgreSQL view.

**Parameters:**
- `view_name` (str): Name of the view to query
- `where` (dict | None): Filter conditions
- `**kwargs`: Additional filter conditions

**Returns:** Dictionary representing the row, or `None` if not found

**Examples:**

```python
# By ID
user = await repo.find_one("v_user", where={"id": user_id})

# Kwargs style
user = await repo.find_one("v_user", id=user_id)

# By unique field
user = await repo.find_one("v_user", email="john@example.com")

# Multiple conditions
admin = await repo.find_one(
    "v_user",
    where={"email": "john@example.com", "role": "admin"}
)

# Handle not found
user = await repo.find_one("v_user", id="nonexistent-id")
if user is None:
    raise UserNotFoundError()
```

**Best Practices:**
- Always check for `None` return value
- Use for queries that should return zero or one result
- Prefer `find_one()` over `find()[0]` for clarity and safety

---

### `count()`

**Signature:**
```python
async def count(
    view_name: str,
    where: dict | None = None,
    **kwargs
) -> int
```

**Description:** Count records matching the given filters.

**Parameters:**
- `view_name` (str): Name of the view
- `where` (dict | None): Filter conditions
- `**kwargs`: Additional filter conditions

**Returns:** Integer count of matching records

**Examples:**

```python
# Total count
total_users = await repo.count("v_user")

# Filtered count
active_count = await repo.count("v_user", active=True)

# Complex filter
admin_count = await repo.count(
    "v_user",
    where={"role": "admin", "created_at__gte": "2024-01-01"}
)

# Pagination metadata
total = await repo.count("v_user")
page_count = (total + page_size - 1) // page_size
```

---

## Command Methods (CQRS Write Side)

### `insert()`

**Signature:**
```python
async def insert(
    table_name: str,
    data: dict,
    returning: str | list[str] | None = None
) -> dict | Any
```

**Description:** Insert a new record into a table.

**Parameters:**
- `table_name` (str): Name of the table (not view)
- `data` (dict): Column-value pairs to insert
- `returning` (str | list[str] | None): Columns to return after insert

**Returns:**
- If `returning` is a single string: The value of that column
- If `returning` is a list: Dictionary with requested columns
- If `returning` is None: None

**Examples:**

```python
# Insert and get ID
user_id = await repo.insert(
    "users",
    {
        "username": "johndoe",
        "email": "john@example.com",
        "password_hash": hashed_password
    },
    returning="id"
)

# Insert and get multiple fields
result = await repo.insert(
    "posts",
    {
        "title": "New Post",
        "content": "Content here",
        "author_id": author_id
    },
    returning=["id", "created_at"]
)
# result = {"id": "...", "created_at": "..."}

# Simple insert (no return value needed)
await repo.insert(
    "audit_log",
    {
        "user_id": user_id,
        "action": "login",
        "timestamp": datetime.now()
    }
)

# Insert with JSONB data
await repo.insert(
    "products",
    {
        "name": "Widget",
        "data": {"color": "red", "size": "large"}  # JSONB column
    },
    returning="id"
)
```

**Important Notes:**
- Uses table names, not view names
- Automatically handles JSONB serialization
- Returns the value(s) specified in `returning`
- Throws exception on constraint violations (catch and handle)

---

### `update()`

**Signature:**
```python
async def update(
    table_name: str,
    where: dict,
    data: dict,
    returning: str | list[str] | None = None
) -> dict | list[dict] | Any | None
```

**Description:** Update existing record(s) in a table.

**Parameters:**
- `table_name` (str): Name of the table
- `where` (dict): Filter conditions identifying records to update
- `data` (dict): Column-value pairs to update
- `returning` (str | list[str] | None): Columns to return after update

**Returns:**
- Depends on `returning` parameter and number of rows affected
- Returns `None` if no rows matched

**Examples:**

```python
# Update single field
await repo.update(
    "users",
    where={"id": user_id},
    data={"last_login": datetime.now()}
)

# Update multiple fields
updated_user = await repo.update(
    "users",
    where={"id": user_id},
    data={
        "email": new_email,
        "email_verified": False,
        "updated_at": datetime.now()
    },
    returning=["id", "email", "updated_at"]
)

# Conditional update
await repo.update(
    "posts",
    where={"author_id": user_id, "status": "draft"},
    data={"status": "published", "published_at": datetime.now()}
)

# Update with increment
await repo.update(
    "posts",
    where={"id": post_id},
    data={"view_count": "view_count + 1"}  # Raw SQL expression
)

# Bulk update
await repo.update(
    "users",
    where={"role": "beta_tester"},
    data={"role": "user"}
)
```

**Important Notes:**
- Always specify `where` clause (prevent accidental bulk updates)
- Returns `None` if no rows matched the `where` clause
- Can update multiple rows if `where` matches multiple records

---

### `delete()`

**Signature:**
```python
async def delete(
    table_name: str,
    where: dict,
    returning: str | list[str] | None = None
) -> dict | list[dict] | Any | None
```

**Description:** Delete record(s) from a table.

**Parameters:**
- `table_name` (str): Name of the table
- `where` (dict): Filter conditions identifying records to delete
- `returning` (str | list[str] | None): Columns to return from deleted rows

**Returns:**
- Depends on `returning` parameter
- Returns `None` if no rows matched

**Examples:**

```python
# Simple delete
await repo.delete("sessions", where={"id": session_id})

# Delete with return value (soft delete pattern)
deleted_user = await repo.delete(
    "users",
    where={"id": user_id},
    returning=["id", "username", "deleted_at"]
)

# Conditional delete
await repo.delete(
    "tokens",
    where={"expires_at__lt": datetime.now()}
)

# Delete related records (be careful with cascades!)
await repo.delete(
    "comments",
    where={"post_id": post_id}
)

# Prevent accidental full table delete (always use where)
# BAD: await repo.delete("users", where={})  # Deletes everything!
```

**Best Practices:**
- Consider soft deletes (update `deleted_at` instead of DELETE)
- Use `returning` to log what was deleted
- Always specify `where` clause explicitly
- Be aware of CASCADE constraints

---

## Raw SQL Methods

### `execute()`

**Signature:**
```python
async def execute(
    query: str,
    *params
) -> list[dict]
```

**Description:** Execute arbitrary SQL query with parameters.

**Parameters:**
- `query` (str): SQL query string (use `$1`, `$2`, etc. for parameters)
- `*params`: Query parameters (automatically escaped)

**Returns:** List of result rows as dictionaries

**Examples:**

```python
# Custom aggregation
stats = await repo.execute("""
    SELECT
        count(*) as total_users,
        count(*) FILTER (WHERE active = true) as active_users,
        avg(age) as avg_age
    FROM users
""")
# stats = [{"total_users": 100, "active_users": 80, "avg_age": 32.5}]

# Parameterized query (SAFE - prevents SQL injection)
recent_posts = await repo.execute("""
    SELECT * FROM v_post
    WHERE created_at > $1 AND author_id = $2
    ORDER BY created_at DESC
    LIMIT $3
""", since_date, author_id, limit)

# Complex join
results = await repo.execute("""
    SELECT
        u.username,
        count(p.id) as post_count,
        max(p.created_at) as last_post
    FROM users u
    LEFT JOIN posts p ON p.author_id = u.id
    WHERE u.created_at > $1
    GROUP BY u.id, u.username
    HAVING count(p.id) > $2
""", min_signup_date, min_posts)

# Call PostgreSQL function
result = await repo.execute("""
    SELECT * FROM fn_calculate_user_stats($1)
""", user_id)
```

**Important Notes:**
- **Always use parameter placeholders** (`$1`, `$2`) - never string interpolation
- **SQL injection prevention**: Parameters are automatically escaped
- Use for complex queries not supported by other methods
- Consider creating views for frequently used complex queries

---

### `execute_many()`

**Signature:**
```python
async def execute_many(
    query: str,
    params_list: list[tuple]
) -> None
```

**Description:** Execute the same query multiple times with different parameters (bulk operations).

**Parameters:**
- `query` (str): SQL query with parameter placeholders
- `params_list` (list[tuple]): List of parameter tuples

**Returns:** None

**Examples:**

```python
# Bulk insert (more efficient than multiple insert() calls)
users_to_create = [
    ("alice", "alice@example.com"),
    ("bob", "bob@example.com"),
    ("charlie", "charlie@example.com"),
]

await repo.execute_many(
    "INSERT INTO users (username, email) VALUES ($1, $2)",
    users_to_create
)

# Bulk update
updates = [
    (new_role, user_id_1),
    (new_role, user_id_2),
    (new_role, user_id_3),
]

await repo.execute_many(
    "UPDATE users SET role = $1 WHERE id = $2",
    updates
)

# Performance comparison
# Bad: 1000 individual insert() calls = ~1000ms
for user in users:
    await repo.insert("users", user)

# Good: 1 execute_many() call = ~50ms
await repo.execute_many(
    "INSERT INTO users (username, email) VALUES ($1, $2)",
    [(u['username'], u['email']) for u in users]
)
```

**Use Cases:**
- Bulk imports
- Batch processing
- Data migrations
- Significantly faster than individual operations

---

## Transaction Management

### `transaction()`

**Signature:**
```python
async with repo.transaction():
    # All operations within this block are transactional
    ...
```

**Description:** Create a transaction context. All operations within the block are atomic (all succeed or all fail).

**Examples:**

```python
# Transfer funds (atomic operation)
async with repo.transaction():
    # Deduct from sender
    await repo.update(
        "accounts",
        where={"id": sender_id},
        data={"balance": "balance - $1"},
        params=[amount]
    )

    # Add to receiver
    await repo.update(
        "accounts",
        where={"id": receiver_id},
        data={"balance": "balance + $1"},
        params=[amount]
    )

    # Log transaction
    await repo.insert(
        "transactions",
        {
            "from_id": sender_id,
            "to_id": receiver_id,
            "amount": amount
        }
    )
    # If any operation fails, ALL are rolled back

# Complex multi-step operation
async with repo.transaction():
    # Create user
    user_id = await repo.insert("users", user_data, returning="id")

    # Create profile
    await repo.insert("profiles", {"user_id": user_id, ...})

    # Create initial settings
    await repo.insert("settings", {"user_id": user_id, ...})

    # Send welcome email (external API call)
    await send_welcome_email(user_data["email"])
    # If email fails, everything rolls back

# Handle transaction errors
try:
    async with repo.transaction():
        await repo.update("inventory", ...)
        await repo.insert("orders", ...)
except InsufficientInventoryError:
    logger.error("Not enough inventory, transaction rolled back")
```

**Important Notes:**
- Transactions are automatically committed on success
- Transactions are automatically rolled back on exception
- Can nest transactions (creates savepoints)
- Keep transactions short to avoid lock contention

---

## Connection Management

### `close()`

**Signature:**
```python
async def close() -> None
```

**Description:** Close the database connection.

**Example:**

```python
# Manual connection (testing/scripts)
conn = await psycopg.AsyncConnection.connect("postgresql://...")
repo = CQRSRepository(conn)

try:
    # Use repo...
    users = await repo.find("v_user")
finally:
    await repo.close()

# Or with context manager
async with psycopg.AsyncConnection.connect("postgresql://...") as conn:
    repo = CQRSRepository(conn)
    users = await repo.find("v_user")
    # Connection automatically closed
```

**Note:** In FastAPI context, connection management is handled automatically.

---

## Best Practices

### 1. Use Views for Queries

```python
# ✅ GOOD: Query optimized view
users = await repo.find("v_user_with_stats")

# ❌ BAD: Complex join in application code
users = await repo.find("users")
for user in users:
    stats = await repo.find("stats", user_id=user.id)
    user["stats"] = stats
```

### 2. Use Functions for Complex Commands

```python
# ✅ GOOD: Business logic in PostgreSQL function
result = await repo.execute("SELECT * FROM fn_create_order($1, $2)", user_id, items)

# ❌ BAD: Complex business logic in Python
async with repo.transaction():
    order_id = await repo.insert("orders", ...)
    for item in items:
        await repo.insert("order_items", ...)
        await repo.update("inventory", ...)
    # Complex logic prone to bugs
```

### 3. Always Use Parameter Placeholders

```python
# ✅ GOOD: Safe from SQL injection
users = await repo.execute(
    "SELECT * FROM users WHERE email = $1",
    user_email
)

# ❌ DANGER: SQL injection vulnerability!
users = await repo.execute(
    f"SELECT * FROM users WHERE email = '{user_email}'"
)
```

### 4. Handle None Returns

```python
# ✅ GOOD: Check for None
user = await repo.find_one("v_user", id=user_id)
if user is None:
    raise UserNotFoundError(f"User {user_id} not found")

# ❌ BAD: Will raise AttributeError if not found
user = await repo.find_one("v_user", id=user_id)
return user["email"]  # Crashes if user is None!
```

### 5. Use Transactions for Multi-Step Operations

```python
# ✅ GOOD: Atomic operation
async with repo.transaction():
    await repo.update("accounts", ...)
    await repo.insert("transactions", ...)

# ❌ BAD: Can leave inconsistent state
await repo.update("accounts", ...)  # Might fail after this
await repo.insert("transactions", ...)  # Leaving orphaned transaction
```

---

## See Also

- **[CQRS Pattern](../advanced/cqrs.md)** - Architectural pattern
- **[Database Views](../core-concepts/database-views.md)** - Query optimization
- **[Decorators](decorators.md)** - Type and query decorators
- **[Testing](../testing/index.md)** - Repository testing patterns

---

**The `CQRSRepository` is FraiseQL's foundation for clean, type-safe database operations. Master these methods for efficient, maintainable data access.**
