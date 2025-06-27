# Common Errors and Solutions

## Quick Reference

This guide helps you quickly diagnose and fix common FraiseQL errors.

## Table of Contents

1. ['NoneType' object has no attribute 'context'](#nonetype-object-has-no-attribute-context)
2. [Connection already closed](#connection-already-closed)
3. [View must have a 'data' column](#view-must-have-a-data-column)
4. [Cannot instantiate type from view](#cannot-instantiate-type-from-view)
5. [Query returns None/null](#query-returns-nonenull)
6. [Missing required field](#missing-required-field)
7. [Authentication errors](#authentication-errors)
8. [Import errors](#import-errors)

---

## 'NoneType' object has no attribute 'context'

### Error Message
```
AttributeError: 'NoneType' object has no attribute 'context'
```

### Cause
You're using a resolver pattern that FraiseQL doesn't support, typically:
- Using `resolve_` prefix methods
- Using class-based resolvers
- Wrong parameter order in query function

### Solutions

#### ❌ Wrong Pattern 1: resolve_ prefix
```python
# This causes the error
class Query:
    async def resolve_users(self, info):
        # info is None because FraiseQL doesn't use this pattern
        return info.context["db"].find("user_view")  # Error!
```

#### ❌ Wrong Pattern 2: Wrong parameter order
```python
@fraiseql.query
async def user(id: UUID, info) -> User:  # info must be first!
    return info.context["db"].find_one("user_view", id=id)
```

#### ✅ Correct Pattern
```python
import fraiseql

@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view")

@fraiseql.query
async def user(info, id: UUID) -> Optional[User]:
    db = info.context["db"]
    return await db.find_one("user_view", id=id)
```

### Debugging Steps

1. Check you're using `@fraiseql.query` decorator
2. Ensure `info` is the first parameter
3. Remove any `resolve_` prefixes
4. Don't use class-based resolvers

---

## Connection already closed

### Error Message
```
psycopg.OperationalError: the connection is closed
asyncpg.exceptions.ConnectionDoesNotExistError: connection is closed
```

### Cause
- Passing raw database connection instead of using repository
- Connection lifecycle management issues
- Using connection outside of transaction context

### Solutions

#### ❌ Wrong: Direct connection
```python
@fraiseql.query
async def users(info) -> list[User]:
    # Don't create your own connections!
    conn = await asyncpg.connect(DATABASE_URL)
    try:
        result = await conn.fetch("SELECT * FROM users")
        return [User(**r) for r in result]
    finally:
        await conn.close()  # Connection might be reused!
```

#### ✅ Correct: Use repository
```python
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]  # FraiseQLRepository
    return await db.find("user_view")
```

#### For Transactions
```python
@fraiseql.mutation
async def transfer_funds(info, from_id: UUID, to_id: UUID, amount: int) -> bool:
    db = info.context["db"]
    
    # Use the pool for transactions
    async with db._pool.connection() as conn:
        async with conn.transaction():
            # All queries within transaction
            await conn.execute(...)
    
    return True
```

---

## View must have a 'data' column

### Error Message
```
ValueError: View 'user_view' must have a 'data' column containing JSONB
```

### Cause
Your database view doesn't follow FraiseQL's JSONB pattern.

### Solutions

#### ❌ Wrong: Traditional columns
```sql
CREATE VIEW user_view AS
SELECT id, name, email, created_at
FROM users;
```

#### ✅ Correct: JSONB data column
```sql
CREATE VIEW user_view AS
SELECT
    id,              -- For filtering
    email,           -- For lookups
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'created_at', created_at
    ) as data        -- Required!
FROM users;
```

### Checklist
- [ ] View has a column named `data`
- [ ] The `data` column is JSONB type (not JSON or TEXT)
- [ ] The JSONB contains all fields needed for the GraphQL type
- [ ] Filter columns are outside the JSONB

---

## Cannot instantiate type from view

### Error Message
```
TypeError: User.__init__() missing required positional argument: 'email'
ValueError: Cannot instantiate User from view data
```

### Cause
The JSONB data doesn't match the Python type definition.

### Solutions

#### Check Type Definition
```python
@fraiseql.type
class User:
    id: UUID
    email: str        # Required field
    name: str         # Required field
    bio: Optional[str] = None  # Optional field
```

#### Check View Returns All Required Fields
```sql
CREATE VIEW user_view AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'email', email,     -- Required
        'name', name,       -- Required
        'bio', bio          -- Optional, can be null
    ) as data
FROM users;
```

### Common Issues
1. **Missing required fields in JSONB**
2. **Type mismatches** (e.g., returning string for UUID)
3. **Wrong field names** (case sensitive!)
4. **Null values for non-optional fields**

---

## Query returns None/null

### Error Message
No error, but GraphQL returns:
```json
{
  "data": {
    "user": null
  }
}
```

### Causes and Solutions

#### 1. Record doesn't exist
```python
@fraiseql.query
async def user(info, id: UUID) -> Optional[User]:
    db = info.context["db"]
    return await db.find_one("user_view", id=id)  # Returns None if not found
```

#### 2. Missing @fraiseql.query decorator
```python
# ❌ Forgot decorator - won't be exposed!
async def users(info) -> list[User]:
    return []

# ✅ With decorator
@fraiseql.query
async def users(info) -> list[User]:
    return []
```

#### 3. Exception in resolver
```python
@fraiseql.query
async def user(info, id: UUID) -> Optional[User]:
    try:
        db = info.context["db"]
        return await db.find_one("user_view", id=id)
    except Exception as e:
        # Log the error!
        print(f"Error in user query: {e}")
        return None  # Returns null to client
```

---

## Missing required field

### Error in Development Mode (v0.1.0a18+)
```
Warning: Partial instantiation of User - missing fields: ['email', 'created_at']
```

### Error in Production Mode
```
TypeError: User.__init__() missing required argument: 'email'
```

### Cause
GraphQL query doesn't request all required fields, and production mode requires full instantiation.

### Solutions

#### For Development
```graphql
# This works in development even if email is required
query {
  users {
    id
    name
    # email not requested - OK in dev mode
  }
}
```

#### For Production
```graphql
# Must request all required fields
query {
  users {
    id
    name
    email  # Required field must be requested
  }
}
```

#### Make Fields Optional
```python
@fraiseql.type
class User:
    id: UUID
    name: str
    email: Optional[str] = None  # Now optional
```

---

## Authentication errors

### "Not authenticated"
```json
{
  "errors": [{
    "message": "Authentication required"
  }]
}
```

#### Check Authentication Setup
```python
# 1. Ensure auth is configured
app = fraiseql.create_fraiseql_app(
    auth=Auth0Config(...),  # Or your auth provider
    # ...
)

# 2. Send auth header
headers = {
    "Authorization": "Bearer YOUR_TOKEN"
}

# 3. Use requires_auth decorator
@fraiseql.query
@requires_auth
async def me(info) -> User:
    user = info.context["user"]  # Guaranteed to exist
    # ...
```

### "Invalid token"
Check:
1. Token hasn't expired
2. Token is from correct issuer
3. Token has correct audience
4. Algorithms match configuration

---

## Import errors

### "No module named 'fraiseql'"
```bash
# Install FraiseQL
pip install fraiseql
```

### "Cannot import name 'fraise_type'"
```python
# ❌ Old import style
from fraiseql import fraise_type

# ✅ New import style (recommended)
import fraiseql

@fraiseql.type
class User:
    pass

# ✅ Or if you need the old style
from fraiseql import fraise_type, fraise_input, fraise_enum
```

### "Import cycle detected"
```python
# ❌ Circular imports
# user.py
from .post import Post

# post.py  
from .user import User

# ✅ Use forward references
# user.py
@fraiseql.type
class User:
    posts: list['Post']  # String forward reference

# post.py
@fraiseql.type
class Post:
    author: 'User'  # String forward reference
```

---

## General Debugging Tips

### 1. Enable Debug Mode
```python
app = fraiseql.create_fraiseql_app(
    production=False,  # Enables GraphQL playground
    debug=True,        # More detailed errors
    # ...
)
```

### 2. Check Logs
```python
import logging
logging.basicConfig(level=logging.DEBUG)
logger = logging.getLogger(__name__)

@fraiseql.query
async def users(info) -> list[User]:
    logger.debug("Fetching users")
    db = info.context["db"]
    result = await db.find("user_view")
    logger.debug(f"Found {len(result)} users")
    return result
```

### 3. Test Queries in Playground
Visit `http://localhost:8000/graphql` and use the GraphQL playground to test queries interactively.

### 4. Verify Database Views
```sql
-- Check view structure
\d+ user_view

-- Test view directly
SELECT * FROM user_view LIMIT 1;

-- Verify JSONB structure
SELECT jsonb_pretty(data) FROM user_view LIMIT 1;
```

### 5. Use Correct Error Handling
```python
from fraiseql import GraphQLError

@fraiseql.query
async def user(info, id: UUID) -> User:
    db = info.context["db"]
    user = await db.find_one("user_view", id=id)
    
    if not user:
        # Proper GraphQL error
        raise GraphQLError(
            f"User {id} not found",
            extensions={"code": "USER_NOT_FOUND"}
        )
    
    return user
```

## Getting Help

If you're still stuck:

1. Check the [API Reference](../api-reference/index.md)
2. Review [Common Patterns](../COMMON_PATTERNS.md)
3. Look at [Examples](../../examples/)
4. Search [GitHub Issues](https://github.com/fraiseql/fraiseql/issues)
5. Ask in the community