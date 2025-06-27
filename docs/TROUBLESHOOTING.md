# FraiseQL Troubleshooting Guide

## Table of Contents

1. [Common Query Issues](#common-query-issues)
2. [Repository and Database Issues](#repository-and-database-issues)
3. [Input Type Issues](#input-type-issues)
4. [Migration from Traditional GraphQL](#migration-from-traditional-graphql)
5. [Performance Issues](#performance-issues)

## Common Query Issues

### 1. AttributeError: module 'fraiseql' has no attribute 'build_schema'

**Problem**: This function doesn't exist in FraiseQL.

**Solution**: Use `create_fraiseql_app()` instead:

```python
# ❌ Wrong
schema = fraiseql.build_schema(queries=[...])

# ✅ Correct
app = fraiseql.create_fraiseql_app(
    types=[User, Post],
    production=False
)
```

### 2. TypeError: Type Query must define one or more fields

**Problem**: No queries are registered with the schema.

**Solution**: Make sure you:
1. Have at least one `@fraiseql.query` decorated function
2. Import the module containing your queries before creating the app

```python
# ❌ Wrong - queries not imported
app = fraiseql.create_fraiseql_app(types=[User])

# ✅ Correct - import queries first
from my_queries import get_user, list_users  # Import queries
app = fraiseql.create_fraiseql_app(types=[User])
```

### 3. GraphQL Playground not showing up

**Problem**: Playground is disabled in production mode.

**Solution**: Set `production=False` when creating the app:

```python
app = fraiseql.create_fraiseql_app(
    types=[...],
    production=False  # ← This enables Playground
)
```

### 4. ModuleNotFoundError: No module named 'fraiseql'

**Problem**: FraiseQL is not installed.

**Solution**: Install it via pip:

```bash
pip install fraiseql
```

### 5. RuntimeError: No LoaderRegistry in context

**Problem**: DataLoader is being used without proper setup.

**Solution**: Make sure you're using `create_fraiseql_app()` which sets up the context:

```python
# The app automatically sets up LoaderRegistry
app = fraiseql.create_fraiseql_app(
    database_url="postgresql://...",
    types=[...]
)
```

### 6. Query/Mutation not appearing in schema

**Problem**: Functions not properly decorated or registered.

**Solution**: Check that:
1. Function has `@fraiseql.query` or `@fraiseql.mutation` decorator
2. Function has proper type hints for return value
3. Module is imported before app creation

```python
# ✅ Correct query definition
@fraiseql.query
async def get_user(info, id: int) -> Optional[User]:  # ← Return type required
    return User(...)
```

### 7. Invalid type passed to convert_type_to_graphql_input

**Problem**: Using unsupported types in input/output.

**Solution**: Use supported types:
- Basic types: `str`, `int`, `float`, `bool`, `datetime`
- Optional: `Optional[T]`
- Lists: `List[T]`
- FraiseQL types: `@fraiseql.type`, `@fraiseql.input`

### 8. Database connection errors

**Problem**: Can't connect to PostgreSQL.

**Solution**: Check your database URL:

```python
# Format: postgresql://user:password@host:port/database
app = fraiseql.create_fraiseql_app(
    database_url="postgresql://postgres:password@localhost:5432/mydb",
    types=[...]
)
```

### 9. Authentication not working

**Problem**: `@requires_auth` not blocking unauthenticated requests.

**Solution**: Make sure authentication is configured:

```python
from fraiseql.auth.auth0 import Auth0Config

app = fraiseql.create_fraiseql_app(
    types=[...],
    auth=Auth0Config(
        domain="your-domain.auth0.com",
        api_identifier="your-api-id"
    )
)
```

### 10. Type conversion errors

**Problem**: "from_dict() missing" or similar errors.

**Solution**: FraiseQL types should use standard Python types:

```python
# ❌ Wrong - custom class without proper definition
class CustomType:
    pass

# ✅ Correct - use @fraiseql.type
@fraiseql.type
class User:
    id: int
    name: str
```

## Debug Checklist

When something isn't working, check:

1. **Imports**: Are all decorators imported correctly?
   ```python
   import fraiseql
   from fraiseql import fraise_field
   ```

2. **Decorators**: Are types and queries properly decorated?
   ```python
   @fraiseql.type  # For types
   @fraiseql.query  # For queries
   @fraiseql.mutation  # For mutations
   ```

3. **Type hints**: Do all queries/mutations have return type hints?
   ```python
   async def my_query(info) -> ReturnType:  # ← Required
   ```

4. **Registration**: Are types passed to `create_fraiseql_app()`?
   ```python
   app = fraiseql.create_fraiseql_app(
       types=[User, Post, Comment]  # ← All your types
   )
   ```

5. **Server running**: Is the server actually running?
   ```bash
   python your_app.py
   # Should show: "Uvicorn running on http://0.0.0.0:8000"
   ```

## Repository and Database Issues

### 'FraiseQLRepository' object has no attribute 'find'

**Problem**: Getting AttributeError when trying to use repository methods.

**Possible Causes**:
1. Using an older version of FraiseQL
2. Repository not properly instantiated
3. Wrong import or type

**Solution**:
```python
# 1. Check version
pip show fraiseql  # Should be v0.1.0a14+

# 2. Debug repository type
@fraiseql.query
async def debug_repo(info) -> str:
    db = info.context["db"]
    return f"Type: {type(db)}, Has find: {hasattr(db, 'find')}"

# 3. Ensure proper context setup
from fraiseql.db import FraiseQLRepository

async def get_context(request: Request) -> dict[str, Any]:
    pool = request.app.state.db_pool
    repo = FraiseQLRepository(pool, context={"mode": "development"})
    return {"db": repo}
```

### Repository returns None or empty results

**Problem**: Queries return no data even though database has records.

**Possible Causes**:
1. View doesn't have required `data` column
2. Incorrect filters being applied
3. Tenant filtering excluding all records

**Solution**:
```sql
-- Check view structure
SELECT column_name FROM information_schema.columns
WHERE table_name = 'your_view' AND column_name = 'data';

-- Ensure view has proper structure
CREATE VIEW user_view AS
SELECT
    id, email, status,  -- Filtering columns
    jsonb_build_object(
        'id', id,
        'email', email,
        'name', name
    ) as data  -- REQUIRED data column!
FROM users;
```

## Input Type Issues

### 'dict' object has no attribute 'field_name'

**Problem**: AttributeError when accessing fields on input types.

**Cause**: GraphQL sometimes passes input types as dicts instead of typed objects.

**Solution**: Always handle both cases:
```python
@fraiseql.query
async def users(info, where: UserWhereInput | None = None) -> list[User]:
    if where:
        # Safe access pattern
        def get_field(field_name: str):
            if isinstance(where, dict):
                return where.get(field_name)
            else:
                return getattr(where, field_name, None)

        email = get_field('email')
        status = get_field('status')
```

### Where input defined but filtering not working

**Problem**: Queries accept where parameter but don't actually filter results.

**Solution**: Implement filter building logic:
```python
@fraiseql.query
async def machines(info, where: MachineWhereInput | None = None) -> list[Machine]:
    db = info.context["db"]

    # Don't forget to build and use filters!
    filters = _build_filters(where)
    return await db.find("machine_view", **filters)

def _build_filters(where: MachineWhereInput | dict | None) -> dict[str, Any]:
    filters = {}
    if not where:
        return filters

    # Handle both dict and object
    get_field = (lambda f: where.get(f)) if isinstance(where, dict) else (lambda f: getattr(where, f, None))

    # Add each filter
    if get_field('status'):
        filters['status'] = get_field('status')

    return filters
```

## Migration from Traditional GraphQL

### TypeError: Type Query must define one or more fields

**Problem**: Trying to use class-based Query type.

**Cause**: FraiseQL doesn't use resolver classes.

**Solution**: Use function decorators instead:
```python
# ❌ WRONG - Class-based approach
@fraiseql.type
class Query:
    async def users(self, info) -> list[User]:
        pass

# ✅ CORRECT - Function-based approach
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view")
```

### 'NoneType' object has no attribute 'context'

**Problem**: Info parameter is None in query function.

**Possible Causes**:
1. Using `resolve_` prefix on method names
2. Wrong parameter order (info must be first)
3. Using class-based resolvers

**Solution**:
```python
# ❌ WRONG - resolve_ prefix
async def resolve_users(self, info):
    pass

# ❌ WRONG - info not first
async def users(id: int, info) -> User:
    pass

# ✅ CORRECT - @fraiseql.query with info first
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view")
```

## Performance Issues

### Queries returning too much data

**Problem**: Fetching all records then filtering in Python.

**Solution**: Use database-level filtering:
```python
# ❌ WRONG - Fetching everything
@fraiseql.query
async def active_users(info) -> list[User]:
    db = info.context["db"]
    all_users = await db.find("user_view")  # Gets ALL users!
    return [u for u in all_users if u.is_active]  # Filters in Python

# ✅ CORRECT - Filter at database level
@fraiseql.query
async def active_users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view", is_active=True)  # Database filters
```

### Complex filtering is slow

**Problem**: Using custom SQL for every query with complex filters.

**Solution**: Pre-compute common filters in database views:
```sql
CREATE VIEW machine_view AS
SELECT
    id, status, tenant_id,
    -- Pre-compute boolean filters
    (removed_at IS NULL) as is_active,
    (stock_location_id IS NOT NULL) as is_stock,
    -- JSONB data
    jsonb_build_object(...) as data
FROM machines;

-- Add indexes for common filter combinations
CREATE INDEX idx_machine_active ON machines(tenant_id) WHERE removed_at IS NULL;
```

## Getting Help

If you're still stuck:

1. Check the [examples](../examples/) directory for working code
2. Look at the [Quick Start Guide](./QUICKSTART_GUIDE.md)
3. Review the [API Reference](./API_REFERENCE_QUICK.md)
4. Report issues at: https://github.com/fraiseql/fraiseql/issues

## Common Patterns That Work

### Minimal Working Example

```python
import fraiseql
from typing import List

@fraiseql.type
class Item:
    id: int
    name: str

@fraiseql.query
async def items(info) -> List[Item]:
    return [Item(id=1, name="Test")]

app = fraiseql.create_fraiseql_app(
    types=[Item],
    production=False
)

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, port=8000)
```

### With Database

```python
@fraiseql.query
async def get_user(info, id: int) -> Optional[User]:
    db = info.context["db"]
    result = await db.fetch_one(
        "SELECT * FROM users WHERE id = %s", (id,)
    )
    return User(**result) if result else None
```

### With Authentication

```python
from fraiseql.auth import requires_auth

@fraiseql.query
@requires_auth
async def me(info) -> User:
    user_id = info.context["user"].user_id
    # Get user from database
    return user
```
