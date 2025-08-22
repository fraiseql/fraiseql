# Decorators API Reference

Complete reference for all FraiseQL decorators used to define GraphQL schemas, resolvers, and optimizations.

## Query & Mutation Decorators

### @query

```python
@fraiseql.query
def query_function(info, *args, **kwargs) -> ReturnType
```

Marks a function as a GraphQL query resolver. Automatically registers with the schema.

#### Parameters

- `info`: GraphQL resolver info object containing context
- `*args, **kwargs`: Query parameters defined by function signature

#### Returns

The decorated function with GraphQL query metadata.

#### Example

```python
from fraiseql import query, fraise_type
from uuid import UUID

@query
async def get_user(info, id: UUID) -> User:
    """Fetch a user by ID."""
    db = info.context["db"]
    return await db.find_one("users", {"id": id})

@query
async def search_users(
    info,
    name: str | None = None,
    limit: int = 10
) -> list[User]:
    """Search users with optional filters."""
    db = info.context["db"]
    filters = {}
    if name:
        filters["name__icontains"] = name
    return await db.find("users", filters, limit=limit)
```

### @mutation

```python
@fraiseql.mutation(
    function: str | None = None,
    schema: str | None = None,
    context_params: dict[str, str] | None = None
)
def mutation_function(info, *args, **kwargs) -> MutationResult
```

Defines a GraphQL mutation with automatic error handling and result typing.

#### Parameters

- `function`: PostgreSQL function name (defaults to snake_case of class name)
- `schema`: PostgreSQL schema containing the function (defaults to `default_mutation_schema` from config, or "public")
- `context_params`: Maps GraphQL context keys to PostgreSQL function parameter names
- `info`: GraphQL resolver info
- `*args, **kwargs`: Mutation input parameters

#### Returns

Mutation result object with success/error states.

#### Default Schema Configuration

As of v0.1.3, you can configure a default schema for all mutations in your FraiseQLConfig:

```python
from fraiseql import FraiseQLConfig, create_fraiseql_app

config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    default_mutation_schema="app",  # All mutations use this schema by default
)

# Now mutations don't need to specify schema repeatedly
@mutation(function="create_user")  # Uses "app" schema
class CreateUser:
    input: CreateUserInput
    success: CreateUserSuccess
    failure: CreateUserError

# Override when needed
@mutation(function="system_function", schema="public")  # Explicit override
class SystemFunction:
    input: SystemInput
    success: SystemSuccess
    failure: SystemError
```

#### Configuration

Mutations require result types decorated with `@result`, `@success`, and `@failure`:

```python
from fraiseql import mutation, result, success, failure, fraise_type

@result
class CreateUserResult:
    pass

@success
@fraise_type
class CreateUserSuccess(CreateUserResult):
    user: User
    message: str = "User created successfully"

@failure
@fraise_type
class CreateUserError(CreateUserResult):
    code: str
    message: str

@mutation
async def create_user(
    info,
    name: str,
    email: str
) -> CreateUserResult:
    """Create a new user."""
    db = info.context["db"]

    try:
        user = await db.create("users", {
            "name": name,
            "email": email
        })
        return CreateUserSuccess(user=user)
    except IntegrityError:
        return CreateUserError(
            code="DUPLICATE_EMAIL",
            message="Email already exists"
        )
```

### @subscription

```python
@fraiseql.subscription
async def subscription_function(info, *args) -> AsyncIterator[Type]
```

Defines a GraphQL subscription for real-time updates.

#### Requirements

- Must be an async generator function
- Must yield values over time
- WebSocket support required

#### Example

```python
from fraiseql import subscription
import asyncio

@subscription
async def on_user_created(info):
    """Subscribe to new user creation events."""
    pubsub = info.context["pubsub"]

    async for event in pubsub.subscribe("user.created"):
        yield event["user"]

@subscription
async def countdown(info, from_number: int = 10):
    """Countdown subscription example."""
    for i in range(from_number, 0, -1):
        await asyncio.sleep(1)
        yield i
```

## Type Definition Decorators

### @fraise_type

```python
@fraiseql.fraise_type(
    sql_source: str | None = None,
    jsonb_column: str | None = None,
    implements: list[type] | None = None,
    resolve_nested: bool = False
)
class TypeName:
    field1: type
    field2: type
```

Defines a GraphQL object type with automatic field inference and JSON serialization support.

#### Features

- Auto-converts Python types to GraphQL types
- Supports nested types and lists
- Optional fields with `| None`
- Default values
- Computed fields via `@field`
- **Automatic JSON serialization** in GraphQL responses (v0.3.9+)
- `from_dict()` class method for creating instances from dictionaries

#### Parameters

- `sql_source`: Optional table/view name for automatic SQL queries
- `jsonb_column`: JSONB column name (defaults to "data")
- `implements`: List of interfaces this type implements
- `resolve_nested`: Whether nested instances should be resolved separately

#### Example

```python
from fraiseql import fraise_type, field
from datetime import datetime
from uuid import UUID

@fraise_type(sql_source="v_user")
class User:
    id: UUID
    username: str
    email: str
    created_at: datetime
    bio: str | None = None

    @field
    def display_name(self) -> str:
        """Computed display name."""
        return f"@{self.username}"

    @field
    async def post_count(self, info) -> int:
        """Count user's posts."""
        db = info.context["db"]
        return await db.count("posts", {"author_id": self.id})

# The decorator automatically provides JSON serialization support:
user = User(
    id=UUID("12345678-1234-1234-1234-123456789abc"),
    username="johndoe",
    email="john@example.com",
    created_at=datetime.now()
)

# Works in GraphQL responses without additional configuration:
# {
#   "data": {
#     "user": {
#       "id": "12345678-1234-1234-1234-123456789abc",
#       "username": "johndoe",
#       "email": "john@example.com",
#       "createdAt": "2024-01-15T10:30:00"
#     }
#   }
# }

# Also supports creating from dictionaries (e.g., from database):
user_data = {
    "id": "12345678-1234-1234-1234-123456789abc",
    "username": "johndoe",
    "email": "john@example.com",
    "createdAt": "2024-01-15T10:30:00"  # camelCase automatically converted
}
user = User.from_dict(user_data)
```

### @fraise_input

```python
@fraiseql.fraise_input
class InputTypeName:
    field1: type
    field2: type | None = None
```

Defines a GraphQL input type for mutations and queries.

#### Example

```python
from fraiseql import fraise_input

@fraise_input
class CreateUserInput:
    username: str
    email: str
    password: str
    bio: str | None = None

@fraise_input
class UpdateUserInput:
    username: str | None = None
    email: str | None = None
    bio: str | None = None
```

### @fraise_enum

```python
@fraiseql.fraise_enum
class EnumName(Enum):
    VALUE1 = "value1"
    VALUE2 = "value2"
```

Defines a GraphQL enum type.

#### Example

```python
from fraiseql import fraise_enum
from enum import Enum

@fraise_enum
class UserRole(Enum):
    ADMIN = "admin"
    MODERATOR = "moderator"
    USER = "user"
    GUEST = "guest"

@fraise_enum
class PostStatus(Enum):
    DRAFT = "draft"
    PUBLISHED = "published"
    ARCHIVED = "archived"
```

## Authorization Decorators

### @authorize_field

```python
@fraiseql.authorize_field(permission="read:sensitive")
def field_name(self, info) -> type:
    pass
```

Adds field-level authorization to GraphQL fields.

#### Parameters
- `permission` (str): Required permission to access this field
- `roles` (list[str], optional): List of roles allowed to access
- `check_func` (callable, optional): Custom authorization function

#### Example

```python
from fraiseql import fraise_type, authorize_field

@fraise_type
class User:
    id: UUID
    username: str

    @authorize_field(permission="read:email")
    def email(self, info) -> str:
        return self._email

    @authorize_field(roles=["admin", "moderator"])
    def admin_notes(self, info) -> str | None:
        return self._admin_notes

    @authorize_field(check_func=lambda user, info: user.id == info.context.user.id)
    def private_data(self, info) -> dict:
        return self._private_data
```

### @fraise_interface

```python
@fraiseql.fraise_interface
class InterfaceName:
    common_field: type
```

Defines a GraphQL interface that other types can implement.

#### Example

```python
from fraiseql import fraise_interface, fraise_type

@fraise_interface
class Node:
    id: UUID
    created_at: datetime
    updated_at: datetime

@fraise_type
class User(Node):
    username: str
    email: str

@fraise_type
class Post(Node):
    title: str
    content: str
    author_id: UUID
```

## Field Decorators

### @field

```python
@fraiseql.field
def field_method(self, info=None) -> ReturnType
```

Defines a computed field on a type.

#### Parameters

- `self`: The parent object instance
- `info`: Optional GraphQL resolver info

#### Example

```python
@fraise_type
class User:
    first_name: str
    last_name: str

    @field
    def full_name(self) -> str:
        """Computed full name field."""
        return f"{self.first_name} {self.last_name}"

    @field
    async def recent_posts(self, info, limit: int = 5) -> list[Post]:
        """Fetch user's recent posts."""
        db = info.context["db"]
        return await db.find(
            "posts",
            {"author_id": self.id},
            order_by="created_at DESC",
            limit=limit
        )
```

### @dataloader_field

```python
@fraiseql.dataloader_field(
    loader_class=LoaderClass,
    key_field="parent_field_name"
)
async def field_name(self, info) -> ReturnType
```

Implements DataLoader-based field resolution for specific N+1 prevention cases.

**Note**: FraiseQL's recommended approach is to use composable SQL views where complex entities reference the data column of child entity views. This eliminates N+1 queries at the database level through proper view composition.

#### Parameters

- `loader_class`: DataLoader subclass to use
- `key_field`: Field name on parent containing the key
- `description`: Optional field description

#### When to Use DataLoader vs Views

**Prefer SQL Views (Recommended)**:
```sql
-- Composable view with nested data
CREATE VIEW v_user_with_posts AS
SELECT
    u.*,
    jsonb_build_object(
        'posts', (
            SELECT jsonb_agg(p.data)
            FROM v_post p
            WHERE p.author_id = u.id
        )
    ) as data
FROM v_user u;
```

```python
@fraise_type
class UserWithPosts:
    id: UUID
    name: str
    email: str
    posts: list[Post]  # Automatically extracted from data column
```

**Use DataLoader for**:
- External API calls
- Cross-database joins
- Dynamic computations that can't be expressed in SQL

#### Example

```python
from fraiseql import fraise_type, dataloader_field
from fraiseql.optimization import DataLoader

class UserLoader(DataLoader):
    async def batch_load(self, user_ids: list[UUID]) -> list[User | None]:
        users = await db.find("users", {"id__in": user_ids})
        user_map = {u.id: u for u in users}
        return [user_map.get(uid) for uid in user_ids]

@fraise_type
class Post:
    id: UUID
    title: str
    author_id: UUID

    @dataloader_field(UserLoader, key_field="author_id")
    async def author(self, info) -> User | None:
        """Load post author - implementation auto-generated."""
        pass  # Auto-implemented by decorator
```

## Authentication Decorators

### @requires_auth

```python
@fraiseql.requires_auth
async def resolver(info, *args) -> Type
```

Requires authentication for resolver execution.

#### Example

```python
from fraiseql import query, requires_auth

@query
@requires_auth
async def get_my_profile(info) -> User:
    """Get current user's profile."""
    user_context = info.context["user"]
    db = info.context["db"]
    return await db.find_one("users", {"id": user_context.id})
```

### @requires_role

```python
@fraiseql.requires_role("role_name")
async def resolver(info, *args) -> Type
```

Requires specific role for access.

#### Example

```python
from fraiseql import mutation, requires_role

@mutation
@requires_role("admin")
async def delete_user(info, user_id: UUID) -> bool:
    """Admin-only user deletion."""
    db = info.context["db"]
    await db.delete("users", {"id": user_id})
    return True
```

### @requires_permission

```python
@fraiseql.requires_permission("permission_name")
async def resolver(info, *args) -> Type
```

Requires specific permission for access.

#### Example

```python
@mutation
@requires_permission("users:write")
async def update_user(info, id: UUID, data: UpdateUserInput) -> User:
    """Update user with permission check."""
    db = info.context["db"]
    return await db.update("users", {"id": id}, data)
```

## Mutation Result Decorators

### @result

```python
@fraiseql.result
class MutationResult:
    pass
```

Base class for mutation results (union type).

### @success

```python
@fraiseql.success
@fraiseql.fraise_type
class MutationSuccess(MutationResult):
    data: Type
    message: str
```

Marks a type as successful mutation result.

### @failure

```python
@fraiseql.failure
@fraiseql.fraise_type
class MutationError(MutationResult):
    code: str
    message: str
```

Marks a type as error mutation result.

#### Complete Example

```python
from fraiseql import mutation, result, success, failure, fraise_type

@result
class LoginResult:
    pass

@success
@fraise_type
class LoginSuccess(LoginResult):
    token: str
    user: User
    expires_at: datetime

@failure
@fraise_type
class LoginError(LoginResult):
    code: str  # INVALID_CREDENTIALS, ACCOUNT_LOCKED, etc.
    message: str
    retry_after: datetime | None = None

@mutation
async def login(
    info,
    email: str,
    password: str
) -> LoginResult:
    """Authenticate user and return token."""
    db = info.context["db"]

    user = await db.find_one("users", {"email": email})
    if not user or not verify_password(password, user.password_hash):
        return LoginError(
            code="INVALID_CREDENTIALS",
            message="Invalid email or password"
        )

    if user.locked_until and user.locked_until > datetime.now():
        return LoginError(
            code="ACCOUNT_LOCKED",
            message="Account temporarily locked",
            retry_after=user.locked_until
        )

    token = generate_jwt_token(user)
    return LoginSuccess(
        token=token,
        user=user,
        expires_at=datetime.now() + timedelta(hours=24)
    )
```

## Field Configuration

### fraise_field

```python
fraiseql.fraise_field(
    default=value,
    default_factory=callable,
    description="Field description",
    graphql_name="fieldName"
)
```

Configures field metadata and behavior.

#### Parameters

- `default`: Default value for field
- `default_factory`: Factory function for defaults
- `description`: Field description in schema
- `graphql_name`: Custom GraphQL field name
- `init`: Include in `__init__` (default: True)
- `repr`: Include in `__repr__` (default: True)
- `compare`: Include in comparisons (default: True)

#### Example

```python
from fraiseql import fraise_type, fraise_field
from datetime import datetime

@fraise_type
class Post:
    id: UUID
    title: str
    content: str

    created_at: datetime = fraise_field(
        default_factory=datetime.now,
        description="Post creation timestamp"
    )

    view_count: int = fraise_field(
        default=0,
        description="Number of times post has been viewed"
    )

    internal_id: str = fraise_field(
        graphql_name="internalId",
        description="Internal tracking ID"
    )
```

## Decorator Composition

Decorators can be combined for complex behaviors:

```python
from fraiseql import query, requires_auth, requires_role

@query
@requires_auth
@requires_role("moderator")
async def get_flagged_content(
    info,
    limit: int = 20,
    offset: int = 0
) -> list[Post]:
    """Get flagged posts for moderation."""
    db = info.context["db"]
    return await db.find(
        "posts",
        {"flagged": True},
        limit=limit,
        offset=offset
    )
```

## Performance Considerations

| Decorator | Performance Impact | Use When |
|-----------|-------------------|----------|
| `@query` | Minimal | Always for queries |
| `@mutation` | Minimal | Always for mutations |
| `@subscription` | WebSocket overhead | Real-time needed |
| `@field` | Per-field call | Computed values |
| `@dataloader_field` | Batching overhead | External APIs, cross-DB |
| `@requires_auth` | Auth check per call | Security required |

## Best Practices

1. **Type Everything**: Always include type hints for parameters and returns
2. **Use SQL Views**: Prefer composable SQL views for related data over DataLoader
3. **Error Handling**: Use result types for mutations
4. **Documentation**: Include docstrings for schema documentation
5. **Security First**: Apply auth decorators at resolver level
6. **Composition**: Layer decorators for complex requirements

## Common Patterns

### Pagination Pattern

```python
@fraise_input
class PaginationInput:
    limit: int = 10
    offset: int = 0
    order_by: str | None = None

@query
async def list_users(
    info,
    pagination: PaginationInput = PaginationInput()
) -> list[User]:
    db = info.context["db"]
    return await db.find(
        "users",
        limit=pagination.limit,
        offset=pagination.offset,
        order_by=pagination.order_by
    )
```

### Filtering Pattern

```python
@fraise_input
class UserFilter:
    name_contains: str | None = None
    email: str | None = None
    role: UserRole | None = None
    created_after: datetime | None = None

@query
async def search_users(
    info,
    filters: UserFilter | None = None
) -> list[User]:
    db = info.context["db"]
    where = {}

    if filters:
        if filters.name_contains:
            where["name__icontains"] = filters.name_contains
        if filters.email:
            where["email"] = filters.email
        if filters.role:
            where["role"] = filters.role.value
        if filters.created_after:
            where["created_at__gt"] = filters.created_after

    return await db.find("users", where)
```

### Composable Views Pattern (Recommended)

```python
# Define views in PostgreSQL that compose data
"""
CREATE VIEW v_user_full AS
SELECT
    u.id,
    u.name,
    u.email,
    jsonb_build_object(
        'id', u.id,
        'name', u.name,
        'email', u.email,
        'posts', (
            SELECT jsonb_agg(p.data)
            FROM v_post p
            WHERE p.author_id = u.id
        ),
        'comments', (
            SELECT jsonb_agg(c.data)
            FROM v_comment c
            WHERE c.user_id = u.id
        )
    ) as data
FROM users u;
"""

# FraiseQL automatically extracts nested data
@fraise_type
class UserFull:
    id: UUID
    name: str
    email: str
    posts: list[Post]
    comments: list[Comment]

@query
async def get_user_full(info, id: UUID) -> UserFull:
    """Single query fetches complete user with relations."""
    db = info.context["db"]
    return await db.find_one("v_user_full", {"id": id})
```
