# FraiseQL Troubleshooting Guide

## Common Issues and Solutions

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
