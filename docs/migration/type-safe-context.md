# Migrating to Type-Safe GraphQL Context

**Date**: January 4, 2026
**Status**: Available in v1.9.2+
**Benefit**: IDE autocompletion, type checking, cleaner code

---

## Overview

FraiseQL now provides a **type-safe GraphQLContext** that enables IDE autocompletion and eliminates unsafe dictionary key access patterns in resolvers.

### Before (Unsafe)

```python
@fraiseql.query
async def get_user(info, id: str) -> User:
    # ❌ Not type-safe - no IDE help
    db = info.context["db"]
    user = await db.find_one("users", {"id": id})
    return user
```

### After (Type-Safe)

```python
from fraiseql.types.context import GraphQLContext
from graphql import GraphQLResolveInfo

@fraiseql.query
async def get_user(info: GraphQLResolveInfo, id: str) -> User:
    # ✅ Type-safe with full IDE autocompletion
    context: GraphQLContext = info.context
    user = await context.db.find_one("users", {"id": id})
    return user
```

---

## What Changed

### New Exports

FraiseQL now exports two new helpers:

1. **`GraphQLContext`** - Typed dataclass for context
2. **`build_context()`** - Helper to build contexts programmatically

```python
from fraiseql import GraphQLContext, build_context
```

### Backward Compatibility

✅ **Fully backward compatible** - old pattern still works:

```python
# Old pattern still supported
db = info.context["db"]
user = info.context.get("user")
```

No breaking changes. You can migrate gradually.

---

## Migration Guide

### Step 1: Update Type Hints

Add type hints to your resolvers:

```python
from graphql import GraphQLResolveInfo

# Before
async def get_user(info, id: str) -> User:
    ...

# After
async def get_user(info: GraphQLResolveInfo, id: str) -> User:
    ...
```

### Step 2: Cast Context to GraphQLContext

Import and cast the context:

```python
from fraiseql.types.context import GraphQLContext

async def get_user(info: GraphQLResolveInfo, id: str) -> User:
    context: GraphQLContext = info.context
    user = await context.db.find_one("users", {"id": id})
    return user
```

### Step 3: Access Context Fields with Type Safety

Now you get IDE autocompletion for all context fields:

```python
context: GraphQLContext = info.context

# Database access (type-safe)
users = await context.db.query("users", where={"active": True})

# User context (type-safe)
if context.authenticated:
    user_id = context.user.user_id
    user_roles = context.user.roles

# Custom context fields
request_id = context.get_extra("request_id")
```

---

## Practical Examples

### Example 1: Simple Query with Authentication Check

**Before:**
```python
@fraiseql.query
async def get_my_profile(info) -> User:
    user = info.context.get("user")
    if not user:
        raise ValueError("Not authenticated")
    return await info.context["db"].get_by_id("users", user["user_id"])
```

**After:**
```python
from fraiseql.types.context import GraphQLContext
from graphql import GraphQLResolveInfo

@fraiseql.query
async def get_my_profile(info: GraphQLResolveInfo) -> User:
    context: GraphQLContext = info.context
    if not context.authenticated:
        raise ValueError("Not authenticated")
    return await context.db.get_by_id("users", context.user.user_id)
```

**Benefits:**
- IDE shows `context.user.user_id` with autocomplete
- Type checker catches if field doesn't exist
- Clearer intent with `context.authenticated`

### Example 2: Mutation with Transaction

**Before:**
```python
@fraiseql.mutation
async def create_user(info, name: str, email: str) -> User:
    db = info.context["db"]
    user = await db.create("users", {
        "name": name,
        "email": email,
        "created_by": info.context["user"]["user_id"] if info.context.get("user") else None,
    })
    return user
```

**After:**
```python
from fraiseql.types.context import GraphQLContext
from graphql import GraphQLResolveInfo

@fraiseql.mutation
async def create_user(
    info: GraphQLResolveInfo, name: str, email: str
) -> User:
    context: GraphQLContext = info.context
    created_by = context.user.user_id if context.authenticated else None

    user = await context.db.create("users", {
        "name": name,
        "email": email,
        "created_by": created_by,
    })
    return user
```

**Benefits:**
- Clear separation of auth logic
- Fewer runtime errors from missing keys
- IDE catches typos immediately

### Example 3: Using Custom Context Fields

**Before:**
```python
@fraiseql.query
async def get_user(info, id: str) -> User:
    db = info.context["db"]
    request_id = info.context.get("request_id", "unknown")
    # ... manually handle request_id
```

**After:**
```python
from fraiseql.types.context import GraphQLContext
from graphql import GraphQLResolveInfo

@fraiseql.query
async def get_user(info: GraphQLResolveInfo, id: str) -> User:
    context: GraphQLContext = info.context
    request_id = context.get_extra("request_id", "unknown")
    # ... use request_id with IDE help
```

### Example 4: Building Context Programmatically

For testing or non-HTTP environments, use `build_context()`:

```python
from fraiseql import build_context

# In tests
db = FakeRepository()
user = UserContext(user_id="test_user")

context = build_context(
    db=db,
    user=user,
    authenticated=True,
)

# Now use context with type safety
assert context.user.user_id == "test_user"
```

---

## Migration Patterns

### Pattern 1: Gradual Migration

Migrate one resolver at a time:

```python
# Old style (still works)
@fraiseql.query
async def old_resolver(info):
    db = info.context["db"]
    ...

# New style (next refactor)
@fraiseql.query
async def new_resolver(info: GraphQLResolveInfo):
    context: GraphQLContext = info.context
    ...
```

### Pattern 2: Shared Context Usage

Extract common context access into helpers:

```python
def get_current_user(context: GraphQLContext) -> UserContext:
    """Get current user with auth check."""
    if not context.authenticated:
        raise ValueError("Not authenticated")
    return context.user

@fraiseql.query
async def get_profile(info: GraphQLResolveInfo) -> User:
    context: GraphQLContext = info.context
    user = get_current_user(context)
    return await context.db.get_by_id("users", user.user_id)
```

### Pattern 3: Custom Context Extensions

Add custom fields to context:

```python
context = build_context(
    db=db,
    user=user,
    request_id="req_123",
    tenant_id="tenant_abc",
    trace_context=trace_info,
)

# Access with get_extra()
request_id = context.get_extra("request_id")
tenant_id = context.get_extra("tenant_id")
```

---

## Troubleshooting

### Issue: Type Checker Complains About Context Assignment

**Error:**
```
error: Incompatible types in assignment (expression has type "dict[str, Any]", variable has type "GraphQLContext")
```

**Solution:**
Use the type cast operator:

```python
from fraiseql.types.context import GraphQLContext

context: GraphQLContext = info.context  # May need cast in strict mode
# or
context = cast(GraphQLContext, info.context)
```

### Issue: Missing FastAPI Request/Response

In non-HTTP contexts, `request` and `response` will be `None`:

```python
context: GraphQLContext = info.context

# Safe check
if context.request:
    path = context.request.url.path
else:
    path = "unknown"
```

### Issue: IDE Not Showing Autocompletion

Make sure to:
1. Import GraphQLContext: `from fraiseql.types.context import GraphQLContext`
2. Add type hint: `context: GraphQLContext = info.context`
3. Restart your IDE

---

## Reference: GraphQLContext Fields

| Field | Type | Description |
|-------|------|-------------|
| `db` | `CQRSRepository` | Database repository for queries/mutations |
| `user` | `UserContext \| None` | Authenticated user info (None if unauthenticated) |
| `authenticated` | `bool` | Whether user is authenticated |
| `request` | `Any \| None` | FastAPI Request object (HTTP only) |
| `response` | `Any \| None` | FastAPI Response object (HTTP only) |
| `loader_registry` | `LoaderRegistry \| None` | DataLoader registry for batch optimization |
| `config` | `FraiseQLConfig \| None` | FraiseQL configuration object |
| `_extras` | `dict[str, Any]` | Custom context fields |

### Methods

| Method | Purpose |
|--------|---------|
| `get_extra(key, default=None)` | Get custom context field |
| `set_extra(key, value)` | Set custom context field |
| `from_dict(dict)` | Create from dictionary |
| `to_dict()` | Convert to dictionary |

---

## FAQ

**Q: Do I have to migrate existing code?**
A: No, the old pattern still works. Migrate gradually when refactoring.

**Q: Will this break my existing resolvers?**
A: No, backward compatible. Old pattern continues to work.

**Q: What about performance?**
A: No performance impact - it's just a dataclass wrapper at runtime.

**Q: Can I mix old and new patterns?**
A: Yes, you can have some resolvers using type-safe context and others using the old dict pattern.

**Q: How do I test resolvers with GraphQLContext?**
A: Use `build_context()`:
```python
from fraiseql import build_context

context = build_context(
    db=test_db,
    user=test_user,
    authenticated=True,
)
# Now pass context to resolver
```

---

## Related Documentation

- **GraphQLContext API**: See `fraiseql.types.context` module
- **FraiseQL Types**: `docs/reference/types.md`
- **Getting Started**: `docs/getting-started/`
- **Resolvers Guide**: `docs/guides/resolvers.md`

---

## Summary

Type-safe GraphQL context provides:
- ✅ IDE autocompletion for context fields
- ✅ Type checking catches mistakes early
- ✅ Cleaner, more readable resolver code
- ✅ Full backward compatibility
- ✅ Zero performance impact

**Start migrating today!** Pick one resolver and try it out.

---

*Last Updated: January 4, 2026*
