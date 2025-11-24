# Repository Classes - FraiseQLRepository vs CQRSRepository

FraiseQL provides two repository classes for database operations, each designed for different use cases and performance characteristics.

## Quick Comparison

| Feature | FraiseQLRepository | CQRSRepository |
|---------|-------------------|----------------|
| **Status** | ✅ Modern (Recommended) | ⚠️ Legacy |
| **Location** | `fraiseql.db` | `fraiseql.cqrs` |
| **Return Type** | `RustResponseBytes` | Python objects (`dict`, `list`) |
| **Performance** | 🚀 Zero-copy Rust pipeline | Standard Python |
| **Use Case** | GraphQL resolvers | Python logic, utilities |
| **Pipeline** | PostgreSQL → Rust → HTTP | PostgreSQL → Python → GraphQL |
| **Field Projection** | Rust-side (ultra-fast) | Python-side |
| **Type Conversion** | snake_case → camelCase in Rust | Manual in Python |
| **Count Method** | ✅ `count()` returns `int` | ✅ `count()` returns `int` |

## FraiseQLRepository (Modern - Recommended)

**Purpose**: High-performance GraphQL responses with zero-copy Rust pipeline

**Location**: `fraiseql.db.FraiseQLRepository`

**When to Use**:
- ✅ GraphQL query resolvers
- ✅ GraphQL mutation resolvers
- ✅ Any resolver returning data to GraphQL clients
- ✅ Performance-critical operations

**Key Characteristics**:
- Returns `RustResponseBytes` ready for HTTP response
- Zero string operations in Python
- Field projection done in Rust
- Automatic camelCase conversion in Rust
- Minimal memory overhead

### Methods

#### find()
```python
async def find(
    self,
    view_name: str,
    field_name: str | None = None,
    info: Any = None,
    **kwargs: Any
) -> RustResponseBytes
```

**Returns**: `RustResponseBytes` - Optimized GraphQL response ready for HTTP

**Example**:
```python
@fraiseql.query
async def users(info, where: UserWhereInput | None = None) -> list[User]:
    db = info.context["db"]  # FraiseQLRepository
    # Returns RustResponseBytes - GraphQL framework handles conversion
    return await db.find("v_users", where=where)
```

#### find_one()
```python
async def find_one(
    self,
    view_name: str,
    field_name: str | None = None,
    info: Any = None,
    **kwargs: Any
) -> RustResponseBytes | None
```

**Returns**: `RustResponseBytes | None` - Single object or null

**Example**:
```python
@fraiseql.query
async def user(info, id: UUID) -> User | None:
    db = info.context["db"]
    return await db.find_one("v_users", where={"id": {"eq": id}})
```

#### count()
```python
async def count(
    self,
    view_name: str,
    **kwargs: Any
) -> int
```

**Returns**: `int` - Plain integer count

**Example**:
```python
@fraiseql.query
async def users_count(info, where: UserWhereInput | None = None) -> int:
    db = info.context["db"]
    return await db.count("v_users", where=where)  # Returns int directly
```

**Note**: `count()` is the exception - it returns a plain `int` instead of `RustResponseBytes` because count is a simple scalar value that doesn't benefit from the Rust pipeline.

### Why RustResponseBytes?

The Rust pipeline provides dramatic performance improvements:

```python
# Traditional approach (slow)
PostgreSQL → Python dicts → Transform to camelCase → Convert to JSON → GraphQL

# FraiseQL approach (fast)
PostgreSQL → Rust transformation → HTTP bytes (zero Python overhead)
```

**Performance Benefits**:
- ⚡ Zero Python string operations
- ⚡ Zero dict allocations for field data
- ⚡ Parallel transformation in Rust
- ⚡ Direct memory write to HTTP response

## CQRSRepository (Legacy)

**Purpose**: Traditional CQRS pattern with Python object manipulation

**Location**: `fraiseql.cqrs.repository.CQRSRepository`

**When to Use**:
- ⚠️ Legacy code (migrate to `FraiseQLRepository` when possible)
- ✅ Python business logic (not GraphQL)
- ✅ Background jobs that need to manipulate data
- ✅ CLI utilities
- ✅ Data migrations

**Key Characteristics**:
- Returns Python objects (`dict`, `list`)
- Can manipulate data in Python before returning
- Entity-class based API
- Traditional repository pattern

### Methods

#### count()
```python
async def count(
    self,
    entity_class: type[T],
    *,
    where: dict[str, Any] | None = None,
) -> int
```

**Example**:
```python
import fraiseql
from fraiseql import CQRSRepository

@fraiseql.query
async def users_count(info, where: UserWhereInput | None = None) -> int:
    repo = CQRSRepository(info.context["connection"])
    return await repo.count(User, where=where)  # Entity-class based
```

#### find_by_id()
```python
async def find_by_id(
    self,
    entity_class: type[T],
    entity_id: UUID
) -> dict[str, Any] | None
```

#### list_entities()
```python
async def list_entities(
    self,
    entity_class: type[T],
    where: dict[str, Any] | None = None,
    limit: int = 100,
    offset: int = 0,
    order_by: list[tuple[str, str]] | None = None
) -> list[dict[str, Any]]
```

## Migration Guide

### From CQRSRepository to FraiseQLRepository

**Before (Legacy)**:
```python
import fraiseql
from fraiseql import CQRSRepository

@fraiseql.query
async def users(info, where: UserWhereInput | None = None) -> list[User]:
    repo = CQRSRepository(info.context["connection"])
    return await repo.list_entities(User, where=where)  # Returns list[dict]

@fraiseql.query
async def users_count(info, where: UserWhereInput | None = None) -> int:
    repo = CQRSRepository(info.context["connection"])
    return await repo.count(User, where=where)
```

**After (Modern)**:
```python
@fraiseql.query
async def users(info, where: UserWhereInput | None = None) -> list[User]:
    db = info.context["db"]  # FraiseQLRepository
    return await db.find("v_users", where=where)  # Returns RustResponseBytes

@fraiseql.query
async def users_count(info, where: UserWhereInput | None = None) -> int:
    db = info.context["db"]
    return await db.count("v_users", where=where)  # Returns int
```

**Key Changes**:
1. Use `info.context["db"]` instead of creating `CQRSRepository`
2. Pass view names (`"v_users"`) instead of entity classes (`User`)
3. Let the framework handle `RustResponseBytes` → GraphQL conversion
4. Both count methods return `int` - no change needed!

## When to Use Which Repository?

### Use FraiseQLRepository ✅

**GraphQL Resolvers**:
```python
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("v_users")  # Fast! Zero-copy pipeline
```

**Count Queries**:
```python
@fraiseql.query
async def total_users(info) -> int:
    db = info.context["db"]
    return await db.count("v_users")  # Returns int directly
```

### Use CQRSRepository ⚠️

**Background Jobs** (non-GraphQL):
```python
async def cleanup_old_records():
    async with get_db_connection() as conn:
        repo = CQRSRepository(conn)
        old_records = await repo.list_entities(
            OldRecord,
            where={"created_at": {"lt": thirty_days_ago}}
        )
        # Manipulate in Python
        for record in old_records:
            record["status"] = "archived"
            await repo.update("old_record", record)
```

**CLI Utilities**:
```python
# scripts/export_users.py
async def export_users_to_csv():
    async with get_db_connection() as conn:
        repo = CQRSRepository(conn)
        users = await repo.list_entities(User)
        # Write to CSV file
        with open("users.csv", "w") as f:
            write_csv(f, users)  # Need Python dicts
```

## Performance Considerations

### FraiseQLRepository Performance
```
PostgreSQL → Rust → HTTP bytes
~10-50x faster than traditional Python approach
Zero GC pressure
Minimal memory allocations
```

### CQRSRepository Performance
```
PostgreSQL → Python dicts → JSON → GraphQL
Traditional performance
Suitable for non-critical paths
```

## API Consistency

Both repositories support the same filter syntax:

```python
# GraphQL where objects
where = UserWhereInput(status={"eq": "active"})

# Dict-based filters
where = {"status": {"eq": "active"}}

# Both work with either repository
result = await db.count("v_users", where=where)  # FraiseQLRepository
result = await repo.count(User, where=where)      # CQRSRepository
```

## Summary

| Scenario | Repository | Reason |
|----------|-----------|--------|
| GraphQL query resolver | `FraiseQLRepository` | Zero-copy performance |
| GraphQL mutation resolver | `FraiseQLRepository` | Zero-copy performance |
| Count query | `FraiseQLRepository.count()` | Returns `int` directly |
| Background job | `CQRSRepository` | Need Python object manipulation |
| CLI utility | `CQRSRepository` | Need Python object manipulation |
| Data migration | `CQRSRepository` | Need Python object manipulation |

**Default Choice**: Use `FraiseQLRepository` (`info.context["db"]`) for all GraphQL resolvers. Only use `CQRSRepository` when you need to manipulate Python objects outside of GraphQL.

## See Also

- [Database API Reference](database.md) - Complete API documentation
- [Query Patterns](../advanced/database-patterns.md) - Common query patterns
