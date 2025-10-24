# Database API Reference

API reference for FraiseQL database operations.

## FraiseQLRepository

Main repository class for database operations.

### Constructor

```python
from fraiseql.db import FraiseQLRepository
import asyncpg

pool = await asyncpg.create_pool("postgresql://...")
repo = FraiseQLRepository(pool, context=None)
```

**Parameters:**
- `pool`: asyncpg connection pool
- `context`: Optional context dictionary

### Query Methods

#### find()

Find multiple records:

```python
async def find(
    self,
    view_name: str,
    limit: int | None = None,
    offset: int | None = None,
    order_by: dict | None = None,
    where: dict | None = None,
    **kwargs
) -> list[dict]:
    """Find records from a view."""
```

**Example:**
```python
users = await repo.find(
    "users_view",
    limit=10,
    where={"is_active": True},
    order_by={"created_at": "desc"}
)
```

#### find_one()

Find a single record:

```python
async def find_one(
    self,
    view_name: str,
    **kwargs
) -> dict | None:
    """Find one record from a view."""
```

**Example:**
```python
user = await repo.find_one("users_view", id=user_id)
```

### Mutation Methods

#### insert()

Insert a new record:

```python
async def insert(
    self,
    table_name: str,
    data: dict
) -> dict:
    """Insert a record into a table."""
```

**Example:**
```python
user = await repo.insert(
    "users",
    {"name": "John", "email": "john@example.com"}
)
```

#### update()

Update an existing record:

```python
async def update(
    self,
    table_name: str,
    id: Any,
    **updates
) -> dict:
    """Update a record in a table."""
```

**Example:**
```python
user = await repo.update(
    "users",
    id=user_id,
    name="Jane"
)
```

#### delete()

Delete a record:

```python
async def delete(
    self,
    table_name: str,
    id: Any
) -> bool:
    """Delete a record from a table."""
```

**Example:**
```python
deleted = await repo.delete("users", id=user_id)
```

### Transaction Support

Use transactions for ACID guarantees:

```python
async with repo.transaction() as tx:
    await tx.execute("UPDATE ...", ...)
    await tx.execute("INSERT ...", ...)
    # Automatically commits on success
    # Automatically rolls back on exception
```

### Context Management

Pass context to queries:

```python
repo_with_context = FraiseQLRepository(
    pool,
    context={"user_id": current_user_id, "tenant_id": tenant_id}
)

# Context is available in queries
users = await repo_with_context.find("users_view")
```

## WHERE Clause Operators

Supported operators in `where` parameter:

```python
where = {
    "age": {"gte": 18, "lt": 65},  # Greater than or equal, less than
    "status": {"in": ["active", "pending"]},  # IN operator
    "email": {"like": "%@example.com"},  # LIKE operator
    "deleted_at": {"is": None},  # IS NULL
    "score": {"between": [10, 20]},  # BETWEEN
}
```

### Operators

- `eq`: Equal (=)
- `ne`: Not equal (!=)
- `gt`: Greater than (>)
- `gte`: Greater than or equal (>=)
- `lt`: Less than (<)
- `lte`: Less than or equal (<=)
- `in`: IN operator
- `nin`: NOT IN operator
- `like`: LIKE operator
- `ilike`: ILIKE operator (case-insensitive)
- `is`: IS NULL/IS NOT NULL
- `between`: BETWEEN operator

## ORDER BY

Sorting results:

```python
order_by = {
    "created_at": "desc",
    "name": "asc"
}
```

## Related

- [Repository Pattern](../core/README.md)
- [Examples](../../examples/)
- [CQRS Pattern](../architecture/)
