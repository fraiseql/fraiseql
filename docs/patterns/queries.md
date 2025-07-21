# FraiseQL Query Patterns

## The One True Pattern

FraiseQL has a single, consistent pattern for defining queries. This guide explains the pattern and common mistakes to avoid.

## Table of Contents

1. [The Pattern](#the-pattern)
2. [Why This Pattern](#why-this-pattern)
3. [Complete Examples](#complete-examples)
4. [Common Mistakes](#common-mistakes)
5. [Migration from Other Libraries](#migration-from-other-libraries)

---

## The Pattern

### ✅ CORRECT: Use @fraiseql.query

```python
import fraiseql

@fraiseql.query
async def query_name(info, param1: Type1, param2: Type2 = default) -> ReturnType:
    """Query documentation."""
    db = info.context["db"]
    # Query implementation
    return result
```

### Key Rules

1. **Always use `@fraiseql.query` decorator**
2. **First parameter MUST be `info`**
3. **Function should be `async`**
4. **Must have return type annotation**
5. **Access database via `info.context["db"]`**

---

## Why This Pattern

### Traditional GraphQL (What FraiseQL is NOT)

```python
# ❌ This is how Strawberry/Graphene work - NOT FraiseQL!
@strawberry.type
class Query:
    @strawberry.field
    async def users(self, info) -> list[User]:
        # Resolver method on a class
        pass

    async def resolve_posts(self, info) -> list[Post]:
        # resolve_ prefix pattern
        pass
```

### FraiseQL's Approach

```python
# ✅ FraiseQL uses simple functions with decorators
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view")

@fraiseql.query
async def posts(info) -> list[Post]:
    db = info.context["db"]
    return await db.find("post_view")
```

### Benefits

1. **Simpler** - No resolver classes to manage
2. **Explicit** - Clear function-to-query mapping
3. **Testable** - Easy to unit test individual queries
4. **Type-safe** - Full type inference support

---

## Complete Examples

### Example 1: Simple Query

```python
import fraiseql
from uuid import UUID
from typing import Optional

@fraiseql.type
class User:
    id: UUID
    email: str
    name: str
    is_active: bool

@fraiseql.query
async def users(info) -> list[User]:
    """Get all active users."""
    db = info.context["db"]
    return await db.find("user_view", is_active=True)
```

### Example 2: Query with Parameters

```python
@fraiseql.query
async def user(info, id: UUID) -> Optional[User]:
    """Get a specific user by ID."""
    db = info.context["db"]
    user = await db.find_one("user_view", id=id)

    if not user:
        return None

    return user
```

### Example 3: Query with Multiple Parameters

```python
from datetime import datetime

@fraiseql.query
async def users_by_criteria(
    info,
    role: Optional[str] = None,
    is_active: bool = True,
    created_after: Optional[datetime] = None,
    limit: int = 100
) -> list[User]:
    """Get users matching criteria."""
    db = info.context["db"]

    # Build filters
    filters = {"is_active": is_active}
    if role:
        filters["role"] = role
    if created_after:
        filters["created_at__gte"] = created_after

    return await db.find("user_view", limit=limit, **filters)
```

### Example 4: Query with Authentication

```python
from fraiseql.auth import requires_auth

@fraiseql.query
@requires_auth
async def my_profile(info) -> User:
    """Get the current user's profile."""
    user = info.context["user"]  # Guaranteed after @requires_auth
    db = info.context["db"]

    profile = await db.find_one("user_view", id=user.user_id)
    if not profile:
        raise GraphQLError("Profile not found")

    return profile
```

### Example 5: Query with Nested Data

```python
@fraiseql.type
class Post:
    id: UUID
    title: str
    content: str
    author: User
    comments: list['Comment']

@fraiseql.type
class Comment:
    id: UUID
    content: str
    author: User
    created_at: datetime

@fraiseql.query
async def post_with_comments(info, id: UUID) -> Optional[Post]:
    """Get a post with all its comments and authors."""
    db = info.context["db"]

    # The view handles the nesting
    return await db.find_one("post_with_comments_view", id=id)
```

Corresponding view:
```sql
CREATE VIEW post_with_comments_view AS
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id,
        'title', p.title,
        'content', p.content,
        'author', (
            SELECT data FROM user_view WHERE id = p.author_id
        ),
        'comments', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', c.id,
                    'content', c.content,
                    'author', (
                        SELECT data FROM user_view WHERE id = c.author_id
                    ),
                    'created_at', c.created_at
                ) ORDER BY c.created_at
            )
            FROM comments c
            WHERE c.post_id = p.id
        )
    ) as data
FROM posts p;
```

### Example 6: Query with Business Logic

```python
@fraiseql.query
async def recommended_posts(info, limit: int = 10) -> list[Post]:
    """Get personalized post recommendations."""
    db = info.context["db"]
    user = info.context.get("user")

    if not user:
        # Unauthenticated users get popular posts
        return await db.find(
            "popular_posts_view",
            limit=limit
        )

    # Get user preferences
    preferences = await db.find_one(
        "user_preferences_view",
        user_id=user.user_id
    )

    if preferences and preferences.favorite_tags:
        # Get posts matching user's interests
        return await db.find(
            "posts_by_tags_view",
            tags=preferences.favorite_tags,
            limit=limit
        )

    # Default to recent posts
    return await db.find(
        "recent_posts_view",
        limit=limit
    )
```

---

## Common Mistakes

### Mistake 1: Using resolve_ Prefix

```python
# ❌ WRONG - This is Strawberry/Graphene pattern
class Query:
    async def resolve_users(self, info):
        return []

# ✅ CORRECT - Use @fraiseql.query
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view")
```

### Mistake 2: Wrong Parameter Order

```python
# ❌ WRONG - info must be first
@fraiseql.query
async def user(id: UUID, info) -> User:
    pass

# ✅ CORRECT - info is always first
@fraiseql.query
async def user(info, id: UUID) -> User:
    pass
```

### Mistake 3: Missing Return Type

```python
# ❌ WRONG - No return type
@fraiseql.query
async def users(info):
    return []

# ✅ CORRECT - Always specify return type
@fraiseql.query
async def users(info) -> list[User]:
    return []
```

### Mistake 4: Not Using Async

```python
# ❌ WRONG - Should be async
@fraiseql.query
def users(info) -> list[User]:
    pass

# ✅ CORRECT - Always use async
@fraiseql.query
async def users(info) -> list[User]:
    pass
```

### Mistake 5: Direct Database Access

```python
# ❌ WRONG - Don't create your own connection
@fraiseql.query
async def users(info) -> list[User]:
    conn = await asyncpg.connect(DATABASE_URL)
    # ...

# ✅ CORRECT - Use repository from context
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view")
```

### Mistake 6: Forgetting the Decorator

```python
# ❌ WRONG - Missing decorator
async def users(info) -> list[User]:
    # This won't be exposed in GraphQL!
    pass

# ✅ CORRECT - Always use decorator
@fraiseql.query
async def users(info) -> list[User]:
    pass
```

---

## Migration from Other Libraries

### From Strawberry

```python
# Strawberry pattern
@strawberry.type
class Query:
    @strawberry.field
    async def users(self) -> list[User]:
        # Complex resolver logic
        pass

# Convert to FraiseQL
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view")
```

### From Graphene

```python
# Graphene pattern
class Query(graphene.ObjectType):
    users = graphene.List(UserType)

    def resolve_users(self, info):
        return User.objects.all()

# Convert to FraiseQL
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view")
```

### From Apollo Server (JavaScript)

```javascript
// Apollo pattern
const resolvers = {
  Query: {
    users: async (parent, args, context) => {
      return await context.db.users.findAll();
    }
  }
};
```

```python
# Convert to FraiseQL
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view")
```

---

## Best Practices

### 1. Keep Queries Simple

```python
# ✅ Good - Query just fetches data
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view")
```

### 2. Use Views for Complex Logic

```python
# ✅ Good - Let the database handle complexity
@fraiseql.query
async def user_statistics(info, user_id: UUID) -> UserStats:
    db = info.context["db"]
    return await db.find_one("user_statistics_view", user_id=user_id)
```

### 3. Handle Errors Gracefully

```python
@fraiseql.query
async def user(info, id: UUID) -> User:
    db = info.context["db"]
    user = await db.find_one("user_view", id=id)

    if not user:
        raise GraphQLError(f"User {id} not found", extensions={
            "code": "USER_NOT_FOUND"
        })

    return user
```

### 4. Use Type Annotations

```python
# ✅ Good - Full type annotations
@fraiseql.query
async def search_users(
    info,
    query: str,
    limit: int = 20,
    offset: int = 0
) -> list[User]:
    # Implementation
    pass
```

### 5. Document Your Queries

```python
@fraiseql.query
async def active_users(info) -> list[User]:
    """
    Get all active users in the system.

    Returns users where is_active=true, ordered by last_login.
    """
    db = info.context["db"]
    return await db.find("active_users_view")
```

## Summary

Remember: In FraiseQL, queries are just decorated async functions where:
- `info` is always the first parameter
- Database access is via `info.context["db"]`
- Return types are required
- No resolver classes or `resolve_` prefixes

This pattern is simpler, more explicit, and easier to test than traditional GraphQL resolver patterns.
