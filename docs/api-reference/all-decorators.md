# FraiseQL Decorators Reference

Complete reference for all FraiseQL decorators.

## Type Decorators

### @fraise_type

Defines a GraphQL object type.

```python
from fraiseql import fraise_type
from uuid import UUID
from datetime import datetime

@fraise_type
class User:
    id: UUID
    name: str
    email: str
    created_at: datetime

    # Methods become GraphQL fields
    def display_name(self) -> str:
        return f"{self.name} ({self.email})"
```

### @fraise_input

Defines a GraphQL input type.

```python
from fraiseql import fraise_input
from typing import Optional

@fraise_input
class CreateUserInput:
    name: str
    email: str
    password: str
    role: Optional[str] = "user"
```

### @fraise_enum

Defines a GraphQL enum type.

```python
from fraiseql import fraise_enum
from enum import Enum

@fraise_enum
class UserRole(Enum):
    ADMIN = "admin"
    USER = "user"
    GUEST = "guest"
```

### @fraise_interface

Defines a GraphQL interface type.

```python
from fraiseql import fraise_interface, fraise_type

@fraise_interface
class Node:
    id: UUID

@fraise_type(implements=[Node])
class User:
    id: UUID
    name: str
```

## Query Decorators

### @query

Registers a function as a GraphQL query field.

```python
from fraiseql import query
from typing import Optional

@query
async def get_user(info, id: UUID) -> Optional[User]:
    """Get user by ID."""
    db = info.context["db"]
    return await db.get_user(id)

@query
async def search_users(
    info,
    query: str,
    limit: int = 10
) -> list[User]:
    """Search users by name or email."""
    db = info.context["db"]
    return await db.search_users(query, limit)
```

### @field

Defines a GraphQL field within a type class.

```python
from fraiseql import fraise_type, field

@fraise_type
class QueryRoot:
    @field
    async def user(self, root, info, id: UUID) -> Optional[User]:
        """Get user by ID."""
        db = info.context["db"]
        return await db.get_user(id)

    @field
    def stats(self, root, info) -> dict[str, int]:
        """Get system statistics."""
        return {
            "total_users": 1000,
            "active_sessions": 42
        }
```

## Mutation Decorators

### @mutation

Defines a GraphQL mutation with input/success/failure pattern.

```python
from fraiseql import mutation, fraise_input, success, failure

@mutation
class CreateUser:
    input: CreateUserInput
    success: CreateUserSuccess
    failure: CreateUserFailure

    async def execute(self, db, input_data):
        try:
            user = await db.create_user(
                name=input_data.name,
                email=input_data.email
            )
            return CreateUserSuccess(user=user)
        except UserExistsError:
            return CreateUserFailure(
                code="USER_EXISTS",
                message="User already exists"
            )
```

### @success

Marks a type as a mutation success result.

```python
from fraiseql import success, fraise_type

@success
@fraise_type
class CreateUserSuccess:
    user: User
    message: str = "User created successfully"
```

### @failure

Marks a type as a mutation failure result.

```python
from fraiseql import failure, fraise_type

@failure
@fraise_type
class CreateUserFailure:
    code: str
    message: str
    field: Optional[str] = None
```

## Authentication Decorators

### @requires_auth

Requires authentication for a query or mutation.

```python
from fraiseql import requires_auth, query

@query
@requires_auth
async def my_profile(info) -> User:
    """Get current user profile."""
    user_context = info.context["user"]
    db = info.context["db"]
    return await db.get_user(user_context.user_id)
```

### @requires_permission

Requires specific permission for access.

```python
from fraiseql import requires_permission, query

@query
@requires_permission("admin:read")
async def list_all_users(info) -> list[User]:
    """List all users (admin only)."""
    db = info.context["db"]
    return await db.get_all_users()
```

## Decorator Combinations

### Query with Authentication

```python
@query
@requires_auth
async def my_posts(info, limit: int = 10) -> list[Post]:
    """Get current user's posts."""
    user = info.context["user"]
    db = info.context["db"]
    return await db.get_user_posts(user.user_id, limit)
```

### Mutation with Authentication

```python
@mutation
class UpdateProfile:
    input: UpdateProfileInput
    success: UpdateProfileSuccess
    failure: UpdateProfileFailure

    @requires_auth
    async def execute(self, db, input_data, user):
        """Update current user's profile."""
        updated = await db.update_user(
            user_id=user.user_id,
            data=input_data.dict(exclude_unset=True)
        )
        return UpdateProfileSuccess(user=updated)
```

### Complex Type with Methods

```python
@fraise_type
class Post:
    id: UUID
    title: str
    content: str
    author_id: UUID
    tags: list[str]
    metadata: dict[str, Any]

    @field
    async def author(self, root, info) -> User:
        """Resolve post author."""
        db = info.context["db"]
        return await db.get_user(self.author_id)

    @field
    def word_count(self, root, info) -> int:
        """Calculate word count."""
        return len(self.content.split())

    def is_published(self) -> bool:
        """Check if post is published."""
        return self.metadata.get("status") == "published"
```

## Parameter Reference

### Common Parameters

- **info**: GraphQL resolve info containing context, field selection, etc.
- **root**: Parent object (for field resolvers)
- **self**: Instance (for methods and mutations)
- **db**: Database connection (passed to mutation execute)
- **user**: Authenticated user context (when using @requires_auth)

### Type Parameters

- **implements**: List of interfaces for @fraise_type
- **description**: GraphQL description for any decorator

## Best Practices

1. **Use type hints**: Always provide complete type annotations
2. **Document with docstrings**: These become GraphQL descriptions
3. **Handle errors gracefully**: Use success/failure pattern for mutations
4. **Keep resolvers simple**: Complex logic should be in service layers
5. **Use authentication decorators**: Don't manually check auth in resolvers

## See Also

- [Query Decorator](./query-decorator.md) - Detailed @query documentation
- [Mutation Pattern](../mutation_pattern.md) - Mutation design pattern
- [Type System](../type-system.md) - FraiseQL type system overview
