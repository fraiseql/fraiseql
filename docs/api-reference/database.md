# Database API Reference

Complete reference for FraiseQL database operations and repository methods.

## Overview

FraiseQL provides a high-performance database API through the `FraiseQLRepository` class, which is automatically available in GraphQL resolvers via `info.context["db"]`.

```python
@query
async def get_user(info, id: UUID) -> User:
    db = info.context["db"]
    return await db.find_one("v_user", where={"id": id})
```

## Accessing the Database

**In Resolvers**:
```python
db = info.context["db"]  # FraiseQLRepository instance
```

**Repository Instance**: Automatically injected into GraphQL context by FraiseQL

## Query Methods

### find()

**Purpose**: Find multiple records

**Signature**:
```python
async def find(
    view_name: str,
    where: dict | WhereType | None = None,
    limit: int | None = None,
    offset: int | None = None,
    order_by: str | OrderByType | None = None
) -> list[dict[str, Any]]
```

**Parameters**:

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| view_name | str | Yes | Database view or table name |
| where | dict \| WhereType \| None | No | Filter conditions |
| limit | int \| None | No | Maximum number of records to return |
| offset | int \| None | No | Number of records to skip |
| order_by | str \| OrderByType \| None | No | Ordering specification |

**Returns**: List of dictionaries (one per record)

**Examples**:
```python
# Simple query
users = await db.find("v_user")

# With filter
active_users = await db.find("v_user", where={"is_active": True})

# With limit and offset
page_users = await db.find("v_user", limit=20, offset=40)

# With ordering
sorted_users = await db.find("v_user", order_by="created_at DESC")

# Complex filter (dict-based)
filtered_users = await db.find(
    "v_user",
    where={
        "name__icontains": "john",
        "created_at__gte": datetime(2025, 1, 1)
    }
)

# Using typed WhereInput
from fraiseql.types import UserWhere

filtered_users = await db.find(
    "v_user",
    where=UserWhere(
        name={"contains": "john"},
        created_at={"gte": datetime(2025, 1, 1)}
    )
)
```

**Filter Operators** (dict-based):

| Operator | Description | Example |
|----------|-------------|---------|
| `field` | Exact match | `{"status": "active"}` |
| `field__eq` | Equals | `{"age__eq": 25}` |
| `field__neq` | Not equals | `{"status__neq": "deleted"}` |
| `field__gt` | Greater than | `{"age__gt": 18}` |
| `field__gte` | Greater than or equal | `{"age__gte": 18}` |
| `field__lt` | Less than | `{"age__lt": 65}` |
| `field__lte` | Less than or equal | `{"age__lte": 65}` |
| `field__in` | In list | `{"status__in": ["active", "pending"]}` |
| `field__contains` | Contains substring (case-sensitive) | `{"name__contains": "John"}` |
| `field__icontains` | Contains substring (case-insensitive) | `{"name__icontains": "john"}` |
| `field__startswith` | Starts with | `{"email__startswith": "admin"}` |
| `field__endswith` | Ends with | `{"email__endswith": "@example.com"}` |
| `field__isnull` | Is null | `{"deleted_at__isnull": True}` |

### find_one()

**Purpose**: Find a single record

**Signature**:
```python
async def find_one(
    view_name: str,
    where: dict | WhereType | None = None,
    **kwargs
) -> dict[str, Any] | None
```

**Parameters**:

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| view_name | str | Yes | Database view or table name |
| where | dict \| WhereType \| None | No | Filter conditions |
| **kwargs | Any | No | Additional filter conditions (merged with where) |

**Returns**: Dictionary representing the record, or None if not found

**Examples**:
```python
# Find by ID
user = await db.find_one("v_user", where={"id": user_id})

# Using kwargs
user = await db.find_one("v_user", id=user_id)

# Find with complex filter
user = await db.find_one(
    "v_user",
    where={"email": "user@example.com", "is_active": True}
)

# Returns None if not found
user = await db.find_one("v_user", where={"id": "nonexistent"})
if user is None:
    raise GraphQLError("User not found")
```

### find_raw_json()

**Purpose**: Find records and return as raw JSON for direct passthrough (internal use)

**Signature**:
```python
async def find_raw_json(
    view_name: str,
    field_name: str,
    info: Any = None,
    **kwargs
) -> RawJSONResult
```

**Note**: This is an internal optimization method. Use `find()` in normal resolvers.

### find_one_raw_json()

**Purpose**: Find single record as raw JSON for direct passthrough (internal use)

**Signature**:
```python
async def find_one_raw_json(
    view_name: str,
    field_name: str,
    info: Any = None,
    **kwargs
) -> RawJSONResult
```

**Note**: This is an internal optimization method. Use `find_one()` in normal resolvers.

## Pagination Methods

### paginate()

**Purpose**: Cursor-based pagination following Relay specification

**Signature**:
```python
async def paginate(
    view_name: str,
    first: int | None = None,
    after: str | None = None,
    last: int | None = None,
    before: str | None = None,
    filters: dict | None = None,
    order_by: str = "id",
    include_total: bool = True,
    jsonb_extraction: bool | None = None,
    jsonb_column: str | None = None
) -> dict[str, Any]
```

**Parameters**:

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| view_name | str | - | Database view or table name |
| first | int \| None | None | Number of items to fetch forward |
| after | str \| None | None | Cursor to fetch after |
| last | int \| None | None | Number of items to fetch backward |
| before | str \| None | None | Cursor to fetch before |
| filters | dict \| None | None | Filter conditions |
| order_by | str | "id" | Field to order by |
| include_total | bool | True | Include total count in result |
| jsonb_extraction | bool \| None | None | Enable JSONB extraction |
| jsonb_column | str \| None | None | JSONB column name |

**Returns**: Dictionary with edges, page_info, and total_count

**Result Structure**:
```python
{
    "edges": [
        {
            "node": {"id": "...", "name": "...", ...},
            "cursor": "cursor_string"
        },
        ...
    ],
    "page_info": {
        "has_next_page": True,
        "has_previous_page": False,
        "start_cursor": "first_cursor",
        "end_cursor": "last_cursor",
        "total_count": 100
    },
    "total_count": 100
}
```

**Examples**:
```python
# Forward pagination
result = await db.paginate("v_user", first=20)

# With cursor
result = await db.paginate("v_user", first=20, after="cursor_xyz")

# Backward pagination
result = await db.paginate("v_user", last=10, before="cursor_abc")

# With filters
result = await db.paginate(
    "v_user",
    first=20,
    filters={"is_active": True},
    order_by="created_at"
)

# Convert to typed Connection
from fraiseql.types import create_connection

connection = create_connection(result, User)
```

**Note**: Usually accessed via `@connection` decorator rather than directly

## Mutation Methods

### create_one()

**Purpose**: Create a single record

**Signature**:
```python
async def create_one(
    view_name: str,
    data: dict[str, Any]
) -> dict[str, Any]
```

**Note**: Not directly available in current FraiseQLRepository. Use `execute_raw()` or PostgreSQL functions.

**Example Pattern**:
```python
@mutation
async def create_user(info, input: CreateUserInput) -> User:
    db = info.context["db"]
    result = await db.execute_raw(
        "INSERT INTO users (data) VALUES ($1) RETURNING *",
        {"name": input.name, "email": input.email}
    )
    return User(**result[0])
```

### update_one()

**Purpose**: Update a single record

**Signature**:
```python
async def update_one(
    view_name: str,
    where: dict[str, Any],
    updates: dict[str, Any]
) -> dict[str, Any]
```

**Note**: Not directly available in current FraiseQLRepository. Use `execute_raw()` or PostgreSQL functions.

**Example Pattern**:
```python
@mutation
async def update_user(info, id: UUID, input: UpdateUserInput) -> User:
    db = info.context["db"]
    result = await db.execute_raw(
        """
        UPDATE users
        SET data = data || $1::jsonb
        WHERE id = $2
        RETURNING *
        """,
        input.__dict__,
        id
    )
    return User(**result[0])
```

### delete_one()

**Purpose**: Delete a single record

**Signature**:
```python
async def delete_one(
    view_name: str,
    where: dict[str, Any]
) -> bool
```

**Note**: Not directly available in current FraiseQLRepository. Use `execute_raw()` or PostgreSQL functions.

## PostgreSQL Function Execution

### execute_function()

**Purpose**: Execute a PostgreSQL function with JSONB input

**Signature**:
```python
async def execute_function(
    function_name: str,
    input_data: dict[str, Any]
) -> dict[str, Any]
```

**Parameters**:

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| function_name | str | Yes | Fully qualified function name (e.g., 'graphql.create_user') |
| input_data | dict | Yes | Dictionary to pass as JSONB to the function |

**Returns**: Dictionary result from the function

**Examples**:
```python
# Execute mutation function
result = await db.execute_function(
    "graphql.create_user",
    {"name": "John", "email": "john@example.com"}
)

# With schema prefix
result = await db.execute_function(
    "auth.register_user",
    {"email": "user@example.com", "password": "secret"}
)
```

**PostgreSQL Function Format**:
```sql
CREATE OR REPLACE FUNCTION graphql.create_user(input jsonb)
RETURNS jsonb
LANGUAGE plpgsql
AS $$
BEGIN
    -- Function implementation
    RETURN jsonb_build_object(
        'success', true,
        'data', ...
    );
END;
$$;
```

### execute_function_with_context()

**Purpose**: Execute a PostgreSQL function with context parameters

**Signature**:
```python
async def execute_function_with_context(
    function_name: str,
    context_args: list[Any],
    input_data: dict[str, Any]
) -> dict[str, Any]
```

**Parameters**:

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| function_name | str | Yes | Fully qualified function name |
| context_args | list | Yes | List of context arguments (e.g., [tenant_id, user_id]) |
| input_data | dict | Yes | Dictionary to pass as JSONB |

**Returns**: Dictionary result from the function

**Examples**:
```python
# With tenant isolation
result = await db.execute_function_with_context(
    "app.create_location",
    [tenant_id, user_id],
    {"name": "Office", "address": "123 Main St"}
)

# Function signature in PostgreSQL
# CREATE FUNCTION app.create_location(
#     p_tenant_id uuid,
#     p_user_id uuid,
#     input jsonb
# ) RETURNS jsonb
```

**Note**: Automatically called by class-based `@mutation` decorator with `context_params`

## Raw SQL Execution

### execute_raw()

**Purpose**: Execute raw SQL queries

**Signature**:
```python
async def execute_raw(
    query: str,
    *params
) -> list[dict[str, Any]]
```

**Parameters**:

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| query | str | Yes | SQL query with parameter placeholders ($1, $2, etc.) |
| *params | Any | No | Query parameters |

**Returns**: List of dictionaries (query results)

**Examples**:
```python
# Simple query
results = await db.execute_raw("SELECT * FROM users")

# With parameters
results = await db.execute_raw(
    "SELECT * FROM users WHERE id = $1",
    user_id
)

# Complex aggregation
stats = await db.execute_raw(
    """
    SELECT
        count(*) as total_users,
        count(*) FILTER (WHERE is_active) as active_users
    FROM users
    WHERE created_at > $1
    """,
    datetime(2025, 1, 1)
)
```

**Security**: Always use parameterized queries to prevent SQL injection

## Transaction Methods

### run_in_transaction()

**Purpose**: Run operations within a database transaction

**Signature**:
```python
async def run_in_transaction(
    func: Callable[..., Awaitable[T]],
    *args,
    **kwargs
) -> T
```

**Parameters**:

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| func | Callable | Yes | Async function to execute in transaction |
| *args | Any | No | Arguments to pass to func |
| **kwargs | Any | No | Keyword arguments to pass to func |

**Returns**: Result of the function

**Examples**:
```python
async def transfer_funds(conn, source_id, dest_id, amount):
    # Deduct from source
    await conn.execute(
        "UPDATE accounts SET balance = balance - $1 WHERE id = $2",
        amount,
        source_id
    )

    # Add to destination
    await conn.execute(
        "UPDATE accounts SET balance = balance + $1 WHERE id = $2",
        amount,
        dest_id
    )

    return True

# Execute in transaction
@mutation
async def transfer(info, input: TransferInput) -> bool:
    db = info.context["db"]
    return await db.run_in_transaction(
        transfer_funds,
        input.source_id,
        input.dest_id,
        input.amount
    )
```

**Note**: Transaction is automatically rolled back on exception

## Connection Pool

### get_pool()

**Purpose**: Access the underlying connection pool

**Signature**:
```python
def get_pool() -> AsyncConnectionPool
```

**Returns**: psycopg AsyncConnectionPool instance

**Example**:
```python
pool = db.get_pool()
print(f"Pool size: {pool.max_size}")
```

## Context and Session Variables

**Automatic Session Variables**:

FraiseQL automatically sets PostgreSQL session variables from context:

- `app.tenant_id` - From `info.context["tenant_id"]`
- `app.contact_id` - From `info.context["contact_id"]` or `info.context["user"]`

**Usage in PostgreSQL**:
```sql
-- Access session variables in functions
CREATE FUNCTION get_my_data()
RETURNS TABLE(...)
AS $$
BEGIN
    RETURN QUERY
    SELECT *
    FROM data
    WHERE tenant_id = current_setting('app.tenant_id')::uuid;
END;
$$ LANGUAGE plpgsql;
```

**Setting Additional Variables**:
```python
# In custom context provider
async def get_context(request):
    return {
        "db": db,
        "tenant_id": extract_tenant_id(request),
        "contact_id": extract_user_id(request)
    }
```

## Performance Modes

**Repository Modes**:

FraiseQL repository operates in two modes:

1. **Production Mode** (default)
   - Returns raw dictionaries
   - Optimized JSON passthrough
   - Minimal object instantiation

2. **Development Mode**
   - Full type instantiation
   - Enhanced debugging
   - Slower but more developer-friendly

**Mode Selection**:
```python
# Explicit mode setting
context = {
    "db": repository,
    "mode": "production"  # or "development"
}
```

## Best Practices

**Query Optimization**:
```python
# Use specific fields instead of SELECT *
users = await db.find("v_user", where={"is_active": True}, limit=100)

# Use pagination for large datasets
result = await db.paginate("v_user", first=50)

# Use database views for complex queries
# Create view: CREATE VIEW v_user_stats AS SELECT ...
stats = await db.find("v_user_stats")
```

**Error Handling**:
```python
@query
async def get_user(info, id: UUID) -> User | None:
    try:
        db = info.context["db"]
        user = await db.find_one("v_user", where={"id": id})
        if not user:
            return None
        return User(**user)
    except Exception as e:
        logger.error(f"Failed to fetch user {id}: {e}")
        raise GraphQLError("Failed to fetch user")
```

**Security**:
```python
# Always use parameterized queries
results = await db.execute_raw(
    "SELECT * FROM users WHERE email = $1",  # Safe
    email
)

# NEVER do this (SQL injection risk):
# results = await db.execute_raw(f"SELECT * FROM users WHERE email = '{email}'")
```

**Transactions**:
```python
# Use transactions for multi-step operations
async def complex_operation(conn, data):
    # All operations succeed or all fail
    await conn.execute("INSERT INTO table1 ...")
    await conn.execute("UPDATE table2 ...")
    await conn.execute("DELETE FROM table3 ...")

result = await db.run_in_transaction(complex_operation, data)
```

## See Also

- [Queries and Mutations](../core/queries-and-mutations.md) - Using database in resolvers
- [Configuration](../core/configuration.md) - Database configuration options
- [PostgreSQL Functions](../core/database-api.md) - Writing database functions
