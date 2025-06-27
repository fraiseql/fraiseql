# Complete Decorator Reference

## Overview

FraiseQL uses decorators to define GraphQL types, queries, mutations, and other schema elements. This reference covers all available decorators with complete examples and common pitfalls.

## Table of Contents

1. [@fraiseql.type](#fraiseqltype)
2. [@fraiseql.query](#fraiseqlquery)
3. [@fraiseql.mutation](#fraiseqlmutation)
4. [@fraiseql.input](#fraiseinput)
5. [@fraiseql.enum](#fraiseqlenum)
6. [@fraiseql.field](#fraiseqlfield)
7. [@fraiseql.interface](#fraiseqlinterface)
8. [@fraiseql.subscription](#fraiseqlsubscription)
9. [Result Pattern Decorators](#result-pattern-decorators)

---

## @fraiseql.type

Define a GraphQL object type (output type).

### Syntax

```python
@fraiseql.type
class TypeName:
    field_name: FieldType
    optional_field: Optional[FieldType] = None
```

### Parameters

None - the decorator takes no parameters.

### Usage Rules

1. **Must use type annotations** - Every field must have a type annotation
2. **Class name becomes GraphQL type name** - `UserProfile` → `UserProfile` in GraphQL
3. **Can use `fraise_field()` for metadata** - Add descriptions, deprecation, etc.
4. **Supports forward references** - Use quotes for self-referential types

### Complete Example

```python
from datetime import datetime
from typing import Optional
from uuid import UUID
import fraiseql
from fraiseql import fraise_field

@fraiseql.type
class User:
    """A user in the system."""
    id: UUID
    email: str = fraise_field(description="User's email address")
    name: str = fraise_field(description="User's display name")
    created_at: datetime
    updated_at: Optional[datetime] = None
    is_active: bool = fraise_field(default=True, description="Whether user can log in")
    
    # Relationships (resolved separately)
    posts: list['Post'] = fraise_field(description="Posts authored by this user")
    profile: Optional['UserProfile'] = None
```

### Common Mistakes

```python
# ❌ WRONG: Using old import style
from fraiseql import fraise_type
@fraise_type  # This works but is deprecated
class User:
    pass

# ✅ CORRECT: Use @fraiseql.type
import fraiseql
@fraiseql.type
class User:
    pass

# ❌ WRONG: Missing type annotations
@fraiseql.type
class User:
    id = 1  # Error: No type annotation
    name = "John"  # Error: No type annotation

# ✅ CORRECT: Always use type annotations
@fraiseql.type
class User:
    id: int
    name: str

# ❌ WRONG: Using mutable defaults
@fraiseql.type
class User:
    tags: list[str] = []  # Dangerous: Shared between instances!

# ✅ CORRECT: Use fraise_field with factory
@fraiseql.type
class User:
    tags: list[str] = fraise_field(default_factory=list)
```

---

## @fraiseql.query

Define a GraphQL query field. This is the primary way to expose data fetching in FraiseQL.

### Syntax

```python
@fraiseql.query
async def query_name(info, param1: Type1, param2: Type2 = default) -> ReturnType:
    # Query implementation
    pass
```

### Parameters

None - the decorator takes no parameters.

### Usage Rules

1. **First parameter MUST be `info`** - Contains context and GraphQL metadata
2. **Must be async** - All queries should be async functions
3. **Must have return type annotation** - Tells GraphQL what type to expect
4. **Function name becomes field name** - `get_users` → `getUsers` in GraphQL (with camelCase conversion)

### Complete Examples

#### Basic Query

```python
@fraiseql.query
async def users(info) -> list[User]:
    """Get all users."""
    db = info.context["db"]
    return await db.find("user_view")
```

#### Query with Parameters

```python
@fraiseql.query
async def user(info, id: UUID) -> Optional[User]:
    """Get a specific user by ID."""
    db = info.context["db"]
    return await db.find_one("user_view", id=id)
```

#### Query with Filtering

```python
@fraiseql.query
async def users_by_role(
    info, 
    role: str,
    is_active: bool = True,
    limit: int = 100
) -> list[User]:
    """Get users filtered by role and status."""
    db = info.context["db"]
    return await db.find(
        "user_view",
        role=role,
        is_active=is_active,
        limit=limit
    )
```

#### Query with Complex Return Type

```python
from fraiseql import PaginatedResponse

@fraiseql.query
async def paginated_users(
    info,
    page: int = 1,
    per_page: int = 20,
    filter: Optional[UserFilter] = None
) -> PaginatedResponse[User]:
    """Get paginated users with optional filtering."""
    db = info.context["db"]
    
    # Implementation details...
    users = await db.find("user_view", limit=per_page, offset=(page-1)*per_page)
    total = await db.count("user_view")
    
    return PaginatedResponse(
        items=users,
        total=total,
        page=page,
        per_page=per_page
    )
```

### Common Mistakes

```python
# ❌ WRONG: Using resolve_ prefix (Strawberry pattern)
class Query:
    async def resolve_users(self, info):
        # This will NOT work in FraiseQL!
        pass

# ✅ CORRECT: Use @fraiseql.query decorator
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view")

# ❌ WRONG: Wrong parameter order
@fraiseql.query
async def user(id: UUID, info) -> User:  # info must be first!
    pass

# ✅ CORRECT: info is always first
@fraiseql.query
async def user(info, id: UUID) -> User:
    pass

# ❌ WRONG: Not async
@fraiseql.query
def users(info) -> list[User]:  # Should be async
    pass

# ✅ CORRECT: Always use async
@fraiseql.query
async def users(info) -> list[User]:
    pass

# ❌ WRONG: Missing return type
@fraiseql.query
async def users(info):  # No return type annotation
    pass

# ✅ CORRECT: Always specify return type
@fraiseql.query
async def users(info) -> list[User]:
    pass
```

---

## @fraiseql.mutation

Define a GraphQL mutation for modifying data.

### Syntax

```python
@fraiseql.mutation
async def mutation_name(info, input: InputType) -> ReturnType:
    # Mutation implementation
    pass
```

### Complete Examples

#### Basic Mutation

```python
@fraiseql.input
class CreateUserInput:
    email: str
    name: str
    password: str

@fraiseql.mutation
async def create_user(info, input: CreateUserInput) -> User:
    """Create a new user."""
    db = info.context["db"]
    
    # In production mode, use PostgreSQL function
    result = await db.call_function(
        "create_user",
        email=input.email,
        name=input.name,
        password_hash=hash_password(input.password)
    )
    
    return User(**result)
```

#### Mutation with Result Union

```python
@fraiseql.result
class CreateUserResult:
    pass

@fraiseql.success
class CreateUserSuccess:
    user: User
    message: str = "User created successfully"

@fraiseql.failure  
class CreateUserError:
    message: str
    code: str
    field: Optional[str] = None

@fraiseql.mutation
async def create_user_safe(
    info, 
    input: CreateUserInput
) -> Union[CreateUserSuccess, CreateUserError]:
    """Create user with error handling."""
    db = info.context["db"]
    
    try:
        # Validate input
        if not is_valid_email(input.email):
            return CreateUserError(
                message="Invalid email format",
                code="INVALID_EMAIL",
                field="email"
            )
        
        # Create user
        user_data = await db.call_function(
            "create_user",
            email=input.email,
            name=input.name
        )
        
        return CreateUserSuccess(user=User(**user_data))
        
    except UniqueViolationError:
        return CreateUserError(
            message="Email already exists",
            code="DUPLICATE_EMAIL",
            field="email"
        )
```

---

## @fraiseql.input

Define a GraphQL input type for mutation arguments.

### Syntax

```python
@fraiseql.input
class InputTypeName:
    field: FieldType
    optional_field: Optional[FieldType] = None
```

### Complete Example

```python
@fraiseql.input
class UpdateUserInput:
    """Input for updating a user."""
    id: UUID
    name: Optional[str] = None
    email: Optional[str] = None
    is_active: Optional[bool] = None
    
    # Nested input types are supported
    profile: Optional['UpdateProfileInput'] = None

@fraiseql.input
class UpdateProfileInput:
    """Input for updating user profile."""
    bio: Optional[str] = None
    avatar_url: Optional[str] = None
    preferences: Optional[dict[str, Any]] = None  # JSON field
```

### Usage in Mutations

```python
@fraiseql.mutation
async def update_user(info, input: UpdateUserInput) -> User:
    """Update user with partial data."""
    db = info.context["db"]
    
    # Build update data, excluding None values
    update_data = {
        k: v for k, v in input.__dict__.items() 
        if v is not None and k != 'id'
    }
    
    result = await db.call_function(
        "update_user",
        user_id=input.id,
        updates=update_data
    )
    
    return User(**result)
```

---

## @fraiseql.enum

Define a GraphQL enum type.

### Syntax

```python
from enum import Enum

@fraiseql.enum
class EnumName(Enum):
    VALUE1 = "value1"
    VALUE2 = "value2"
```

### Complete Example

```python
from enum import Enum

@fraiseql.enum
class UserRole(Enum):
    """User roles in the system."""
    ADMIN = "admin"
    USER = "user"
    MODERATOR = "moderator"
    GUEST = "guest"

@fraiseql.enum
class PostStatus(Enum):
    """Post publication status."""
    DRAFT = "draft"
    PUBLISHED = "published"
    ARCHIVED = "archived"
    DELETED = "deleted"

# Using enums in types
@fraiseql.type
class User:
    id: UUID
    email: str
    role: UserRole = fraise_field(
        default=UserRole.USER,
        description="User's role in the system"
    )

# Using enums in queries
@fraiseql.query
async def users_by_role(info, role: UserRole) -> list[User]:
    """Get all users with a specific role."""
    db = info.context["db"]
    return await db.find("user_view", role=role.value)
```

---

## @fraiseql.field

Add custom field resolvers to types. Used for computed fields or complex data fetching.

### Syntax

```python
@fraiseql.type
class TypeName:
    # Regular fields
    id: int
    
    @fraiseql.field
    async def computed_field(self, info) -> FieldType:
        # Custom resolution logic
        pass
```

### Complete Examples

#### Computed Field

```python
@fraiseql.type
class User:
    id: UUID
    first_name: str
    last_name: str
    
    @fraiseql.field
    async def full_name(self, info) -> str:
        """Computed full name."""
        return f"{self.first_name} {self.last_name}"
    
    @fraiseql.field
    async def post_count(self, info) -> int:
        """Count of user's posts."""
        db = info.context["db"]
        return await db.count("post_view", author_id=self.id)
```

#### Related Data Field

```python
@fraiseql.type
class User:
    id: UUID
    email: str
    
    @fraiseql.field
    async def recent_posts(self, info, limit: int = 5) -> list[Post]:
        """Get user's recent posts."""
        db = info.context["db"]
        return await db.find(
            "post_view",
            author_id=self.id,
            order_by="created_at DESC",
            limit=limit
        )
```

---

## @fraiseql.interface

Define a GraphQL interface that other types can implement.

### Syntax

```python
@fraiseql.interface
class InterfaceName:
    # Common fields all implementations must have
    field: FieldType
```

### Complete Example

```python
@fraiseql.interface
class Node:
    """An object with an ID."""
    id: UUID

@fraiseql.interface
class Timestamped:
    """An object with timestamps."""
    created_at: datetime
    updated_at: Optional[datetime]

# Types can implement interfaces
@fraiseql.type
class User(Node, Timestamped):
    """User implements Node and Timestamped."""
    id: UUID  # From Node
    created_at: datetime  # From Timestamped
    updated_at: Optional[datetime]  # From Timestamped
    # User-specific fields
    email: str
    name: str

@fraiseql.type
class Post(Node, Timestamped):
    """Post implements Node and Timestamped."""
    id: UUID  # From Node
    created_at: datetime  # From Timestamped
    updated_at: Optional[datetime]  # From Timestamped
    # Post-specific fields
    title: str
    content: str
```

---

## @fraiseql.subscription

Define a GraphQL subscription for real-time updates.

### Syntax

```python
@fraiseql.subscription
async def subscription_name(info, param: Type) -> AsyncIterator[ReturnType]:
    # Subscription implementation
    yield value
```

### Complete Example

```python
from typing import AsyncIterator
import asyncio

@fraiseql.subscription
async def post_created(info, author_id: Optional[UUID] = None) -> AsyncIterator[Post]:
    """Subscribe to new posts, optionally filtered by author."""
    # Get pubsub from context
    pubsub = info.context["pubsub"]
    
    # Subscribe to channel
    channel = f"posts:{author_id}" if author_id else "posts:all"
    subscription = await pubsub.subscribe(channel)
    
    try:
        async for message in subscription:
            post_data = json.loads(message)
            yield Post(**post_data)
    finally:
        await pubsub.unsubscribe(channel)

# In your mutation, publish events
@fraiseql.mutation
async def create_post(info, input: CreatePostInput) -> Post:
    """Create a post and notify subscribers."""
    db = info.context["db"]
    pubsub = info.context["pubsub"]
    
    # Create post
    post_data = await db.call_function("create_post", **input.__dict__)
    post = Post(**post_data)
    
    # Publish to subscribers
    await pubsub.publish("posts:all", json.dumps(post_data))
    await pubsub.publish(f"posts:{post.author_id}", json.dumps(post_data))
    
    return post
```

---

## Result Pattern Decorators

FraiseQL provides decorators for implementing the Result pattern for better error handling.

### @fraiseql.result

Mark a base class for a result union.

```python
@fraiseql.result
class MutationResult:
    """Base class for mutation results."""
    pass
```

### @fraiseql.success

Mark a success variant of a result.

```python
@fraiseql.success
class CreateUserSuccess:
    user: User
    message: str = "User created successfully"
```

### @fraiseql.failure

Mark a failure variant of a result.

```python
@fraiseql.failure
class CreateUserError:
    message: str
    code: str
    field: Optional[str] = None
```

### Complete Result Pattern Example

```python
# Define result types
@fraiseql.result
class LoginResult:
    pass

@fraiseql.success
class LoginSuccess:
    user: User
    token: str
    expires_at: datetime

@fraiseql.failure
class LoginError:
    message: str
    code: str  # INVALID_CREDENTIALS, ACCOUNT_LOCKED, etc.

# Use in mutation
@fraiseql.mutation
async def login(
    info, 
    email: str, 
    password: str
) -> Union[LoginSuccess, LoginError]:
    """Authenticate user and return token."""
    db = info.context["db"]
    
    # Find user
    user = await db.find_one("user_view", email=email)
    if not user:
        return LoginError(
            message="Invalid email or password",
            code="INVALID_CREDENTIALS"
        )
    
    # Check password
    if not verify_password(password, user.password_hash):
        return LoginError(
            message="Invalid email or password",
            code="INVALID_CREDENTIALS"
        )
    
    # Check if account is active
    if not user.is_active:
        return LoginError(
            message="Account has been deactivated",
            code="ACCOUNT_LOCKED"
        )
    
    # Generate token
    token, expires_at = generate_token(user.id)
    
    return LoginSuccess(
        user=user,
        token=token,
        expires_at=expires_at
    )
```

## Summary

### Key Points to Remember

1. **Always use `@fraiseql.` prefix** - Not `@fraise_` (deprecated)
2. **`info` is always first parameter** - In queries, mutations, and field resolvers
3. **Use type annotations** - Required for all fields and return types
4. **Queries use `@fraiseql.query`** - Never `resolve_` methods
5. **Return types matter** - They define the GraphQL schema

### Import Pattern

```python
# Recommended import style
import fraiseql
from fraiseql import fraise_field

# Then use decorators with fraiseql prefix
@fraiseql.type
@fraiseql.query
@fraiseql.mutation
# etc.
```

### Next Steps

- Learn about [Repository Patterns](./repository.md)
- Understand [Context Management](./context.md)
- Explore [Common Patterns](../patterns/index.md)