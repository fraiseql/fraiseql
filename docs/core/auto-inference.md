# Auto-Inference in FraiseQL

FraiseQL automatically infers several parameters to reduce boilerplate and make your code cleaner. This guide explains what's auto-inferred and when you need to be explicit.

## Overview

FraiseQL auto-infers:
1. **`field_name`** - GraphQL field name from function name
2. **`info`** - GraphQL info parameter from context
3. **`where`, `order_by`, `limit`, `offset`** - Added automatically to list queries
4. **Mutation success fields** - `status`, `message`, `updated_fields`, `id`

---

## 1. field_name Auto-Inference

### How It Works

When you call `db.find()` or `db.find_one()`, FraiseQL automatically extracts the GraphQL field name from `info.field_name`:

```python
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    # field_name is automatically "users" (from function name)
    return await db.find("v_user")
```

**GraphQL Response**:
```json
{
  "data": {
    "users": [...]  // ← Matches function name
  }
}
```

### When Field Name Matters

The `field_name` parameter wraps the response in the GraphQL structure:

```python
# Function name: users
# field_name inferred: "users"
# Response: {"data": {"users": [...]}}

# Function name: activeUsers
# field_name inferred: "activeUsers"
# Response: {"data": {"activeUsers": [...]}}
```

### Manual Override (Rarely Needed)

You can explicitly set `field_name` if needed:

```python
@fraiseql.query
async def get_all_users(info) -> list[User]:
    db = info.context["db"]
    # Explicitly set field_name to "users" instead of "get_all_users"
    return await db.find("v_user", field_name="users")
```

**When to override:**
- Function name doesn't match desired GraphQL field name
- Using helper functions that aren't direct GraphQL resolvers

---

## 2. info Parameter Auto-Injection

### Current State (v1.9.0)

The `info` parameter must be explicitly passed:

```python
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    # info parameter is used but must be explicitly passed
    return await db.find("v_user")
```

### Future State (After PR #200)

The `info` parameter will be auto-injected from the GraphQL context:

```python
@fraiseql.query
async def users() -> list[User]:  # No info parameter needed!
    db = context["db"]
    return await db.find("v_user")
```

**Note**: This documentation describes the future state. Check the current version to see if PR #200 is merged.

---

## 3. Query Parameters Auto-Wiring

### Automatic WHERE, ORDER BY, LIMIT, OFFSET

For list queries, FraiseQL automatically adds these parameters:

```python
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("v_user")
```

**GraphQL automatically supports**:

```graphql
query {
  users(
    where: {isActive: true}
    orderBy: {field: "createdAt", direction: DESC}
    limit: 10
    offset: 20
  ) {
    id
    name
    email
  }
}
```

### How It Works

1. FraiseQL detects `list[T]` return type
2. Automatically adds `where`, `orderBy`, `limit`, `offset` to GraphQL schema
3. Passes these parameters to `db.find()` as `**kwargs`

### Manual Parameter Handling

You can accept and customize these parameters:

```python
@fraiseql.query
async def users(
    info,
    where: dict | None = None,
    limit: int = 100
) -> list[User]:
    db = info.context["db"]

    # Customize or validate parameters
    if limit > 1000:
        limit = 1000  # Cap at 1000

    return await db.find("v_user", where=where, limit=limit)
```

---

## 4. Mutation Success Fields Auto-Injection

### @success Decorator

The `@success` decorator automatically adds standard fields:

```python
@fraiseql.success
class UserCreated:
    user: User  # Your custom field
    # Auto-injected fields (don't add these manually):
    # status: str = "success"
    # message: str | None = None
    # updated_fields: list[str] | None = None
    # id: UUID | None = None
```

**GraphQL Response**:
```json
{
  "data": {
    "createUser": {
      "status": "success",       // ← Auto-injected
      "message": null,           // ← Auto-injected
      "user": {...},             // ← Your field
      "updatedFields": null,     // ← Auto-injected
      "id": null                 // ← Auto-injected
    }
  }
}
```

### Customizing Auto-Injected Fields

You can override default values:

```python
@fraiseql.success
class UserCreated:
    user: User
    message: str = "User created successfully"  # Custom default
    # Other fields still auto-injected
```

### When to Use Auto-Injection

**✅ Use auto-injection for:**
- Standard CRUD operations
- Simple success/error responses
- Following GraphQL best practices

**❌ Don't use auto-injection when:**
- You need full control over response structure
- Building non-standard mutation responses

---

## 5. Comparison with Other Frameworks

### Strawberry (Python)

```python
import strawberry

@strawberry.type
class Query:
    @strawberry.field
    async def users(self) -> list[User]:
        # Function name "users" → GraphQL field name
        return await db.query(User).all()
```

**FraiseQL equivalent**:
```python
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    # Same auto-inference from function name
    return await db.find("v_user")
```

### GraphQL-Python (Graphene)

```python
import graphene

class Query(graphene.ObjectType):
    users = graphene.List(User)

    def resolve_users(self, info):
        # Field name from attribute "users"
        # Resolver is "resolve_" + field_name
        return User.objects.all()
```

**FraiseQL equivalent**:
```python
@fraiseql.query
async def users(info) -> list[User]:
    # Simpler - no separate resolver pattern
    db = info.context["db"]
    return await db.find("v_user")
```

### Ariadne (Python)

```python
from ariadne import QueryType

query = QueryType()

@query.field("users")  # Must specify field name
async def resolve_users(obj, info):
    return await get_users()
```

**FraiseQL equivalent**:
```python
@fraiseql.query
async def users(info) -> list[User]:
    # No need to specify field name twice!
    db = info.context["db"]
    return await db.find("v_user")
```

---

## Auto-Inference Decision Matrix

| Feature | Auto-Inferred? | When to Override |
|---------|----------------|------------------|
| **field_name** | ✅ Yes (from function name) | Helper functions, non-standard naming |
| **info parameter** | ⚠️ Soon (PR #200) | Currently must pass explicitly |
| **where/orderBy/limit/offset** | ✅ Yes (for list queries) | Custom validation, business logic |
| **@success fields** | ✅ Yes (status, message, etc.) | Non-standard mutation responses |

---

## Best Practices

### 1. Trust Auto-Inference

**✅ DO:**
```python
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("v_user")  # Clean, simple
```

**❌ DON'T:**
```python
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    # Unnecessary - field_name is auto-inferred
    return await db.find("v_user", field_name="users", info=info)
```

### 2. Use Explicit Parameters When Needed

**✅ DO:**
```python
@fraiseql.query
async def users(info, limit: int = 100) -> list[User]:
    db = info.context["db"]
    # Explicit limit parameter for validation
    if limit > 1000:
        raise ValueError("Limit cannot exceed 1000")
    return await db.find("v_user", limit=limit)
```

### 3. Follow Naming Conventions

**✅ DO:**
```python
@fraiseql.query
async def users(info) -> list[User]:  # Plural for lists
    ...

@fraiseql.query
async def user(info, id: UUID) -> User | None:  # Singular for single items
    ...
```

**❌ DON'T:**
```python
@fraiseql.query
async def get_user_list(info) -> list[User]:  # Non-standard naming
    return await db.find("v_user", field_name="users")  # Now needs override
```

---

## Debugging Auto-Inference

### Check What Field Name Was Used

```python
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    print(f"Field name: {info.field_name}")  # Debug: See what was inferred
    return await db.find("v_user")
```

### Verify GraphQL Response Structure

Use GraphQL Playground to inspect the response:

```graphql
query {
  users {
    id
    name
  }
}
```

Expected response structure:
```json
{
  "data": {
    "users": [...]  // ← Should match function name
  }
}
```

---

## Summary

FraiseQL's auto-inference reduces boilerplate by 60-70%:

**Traditional GraphQL Framework**:
```python
@query.field("users")
async def resolve_users(obj, info, where=None, limit=100, offset=0, order_by=None):
    # All parameters explicit, field name specified twice
    return await db.find("v_user", "users", info, where, limit, offset, order_by)
```

**FraiseQL with Auto-Inference**:
```python
@fraiseql.query
async def users(info) -> list[User]:
    # Clean, simple, obvious
    return await db.find("v_user")
```

**Result**: Same functionality, 70% less code.

---

## See Also

- [Database API Reference](../reference/database.md) - Complete `find()` and `find_one()` docs
- [Queries and Mutations](queries-and-mutations.md) - Query and mutation patterns
- [Quick Reference](../reference/quick-reference.md) - Common patterns cheatsheet
