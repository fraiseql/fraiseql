# Decorators Reference

Complete reference for all FraiseQL decorators with signatures, parameters, and examples.

## Type Decorators

### @type / @fraise_type

**Purpose**: Define GraphQL object types

**Signature**:
```python
@type(
    sql_source: str | None = None,
    jsonb_column: str | None = "data",
    implements: list[type] | None = None,
    resolve_nested: bool = False
)
```

**Parameters**:

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| sql_source | str \| None | None | Database table/view name for automatic query generation |
| jsonb_column | str \| None | "data" | JSONB column name. Use None for regular column tables |
| implements | list[type] \| None | None | List of GraphQL interface types |
| resolve_nested | bool | False | Resolve nested instances via separate queries |

**Examples**: See [Types and Schema](../core/types-and-schema.md)

### @input / @fraise_input

**Purpose**: Define GraphQL input types

**Signature**:
```python
@input
class InputName:
    field1: str
    field2: int | None = None
```

**Parameters**: None (decorator takes no arguments)

**Examples**: See [Types and Schema](../core/types-and-schema.md)

### @enum / @fraise_enum

**Purpose**: Define GraphQL enum types from Python Enum classes

**Signature**:
```python
@enum
class EnumName(Enum):
    VALUE1 = "value1"
    VALUE2 = "value2"
```

**Parameters**: None

**Examples**: See [Types and Schema](../core/types-and-schema.md)

### @interface / @fraise_interface

**Purpose**: Define GraphQL interface types

**Signature**:
```python
@interface
class InterfaceName:
    field1: str
    field2: int
```

**Parameters**: None

**Examples**: See [Types and Schema](../core/types-and-schema.md)

## Query Decorators

### @query

**Purpose**: Mark async functions as GraphQL queries

**Signature**:
```python
@query
async def query_name(info, param1: Type1, param2: Type2 = default) -> ReturnType:
    pass
```

**Parameters**: None (decorator takes no arguments)

**First Parameter**: Always `info` (GraphQL resolver info)

**Return Type**: Any GraphQL type (fraise_type, list, scalar, Connection, etc.)

**Examples**:
```python
from fraiseql import query

@query
async def get_user(info, id: UUID) -> User:
    db = info.context["db"]
    return await db.find_one("v_user", where={"id": id})

@query
async def search_users(
    info,
    name_filter: str | None = None,
    limit: int = 10
) -> list[User]:
    db = info.context["db"]
    filters = {}
    if name_filter:
        filters["name__icontains"] = name_filter
    return await db.find("v_user", where=filters, limit=limit)
```

**See Also**: [Queries and Mutations](../core/queries-and-mutations.md#query-decorator)

### @connection

**Purpose**: Create cursor-based pagination queries

**Signature**:
```python
@connection(
    node_type: type,
    view_name: str | None = None,
    default_page_size: int = 20,
    max_page_size: int = 100,
    include_total_count: bool = True,
    cursor_field: str = "id",
    jsonb_extraction: bool | None = None,
    jsonb_column: str | None = None
)
```

**Parameters**:

| Parameter | Type | Default | Required | Description |
|-----------|------|---------|----------|-------------|
| node_type | type | - | Yes | Type of objects in the connection |
| view_name | str \| None | None | No | Database view name (inferred from function name if omitted) |
| default_page_size | int | 20 | No | Default number of items per page |
| max_page_size | int | 100 | No | Maximum allowed page size |
| include_total_count | bool | True | No | Include total count in results |
| cursor_field | str | "id" | No | Field to use for cursor ordering |
| jsonb_extraction | bool \| None | None | No | Enable JSONB field extraction (inherits from global config) |
| jsonb_column | str \| None | None | No | JSONB column name (inherits from global config) |

**Must be used with**: @query decorator

**Returns**: Connection[T]

**Examples**:
```python
from fraiseql import connection, query, type
from fraiseql.types import Connection

@type(sql_source="v_user")
class User:
    id: UUID
    name: str

@connection(node_type=User)
@query
async def users_connection(info, first: int | None = None) -> Connection[User]:
    pass  # Implementation handled by decorator

@connection(
    node_type=Post,
    view_name="v_published_posts",
    default_page_size=25,
    max_page_size=50,
    cursor_field="created_at"
)
@query
async def posts_connection(
    info,
    first: int | None = None,
    after: str | None = None
) -> Connection[Post]:
    pass
```

**See Also**: [Queries and Mutations](../core/queries-and-mutations.md#connection-decorator)

## Mutation Decorators

### @mutation

**Purpose**: Define GraphQL mutations

**Function-based Signature**:
```python
@mutation
async def mutation_name(info, input: InputType) -> ReturnType:
    pass
```

**Class-based Signature**:
```python
@mutation(
    function: str | None = None,
    schema: str | None = None,
    context_params: dict[str, str] | None = None,
    error_config: MutationErrorConfig | None = None
)
class MutationName:
    input: InputType
    success: SuccessType
    failure: FailureType
```

**Parameters (Class-based)**:

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| function | str \| None | None | PostgreSQL function name (defaults to snake_case of class name) |
| schema | str \| None | "public" | PostgreSQL schema containing the function |
| context_params | dict[str, str] \| None | None | Maps GraphQL context keys to PostgreSQL function parameters |
| error_config | MutationErrorConfig \| None | None | Configuration for error detection behavior |

**Examples**:
```python
# Function-based
@mutation
async def create_user(info, input: CreateUserInput) -> User:
    db = info.context["db"]
    return await db.create_one("v_user", data=input.__dict__)

# Class-based
@mutation
class CreateUser:
    input: CreateUserInput
    success: CreateUserSuccess
    failure: CreateUserError

# With custom function
@mutation(function="register_new_user", schema="auth")
class RegisterUser:
    input: RegistrationInput
    success: RegistrationSuccess
    failure: RegistrationError

# With context parameters
@mutation(
    function="create_location",
    schema="app",
    context_params={
        "tenant_id": "input_pk_organization",
        "user": "input_created_by"
    }
)
class CreateLocation:
    input: CreateLocationInput
    success: CreateLocationSuccess
    failure: CreateLocationError
```

**See Also**: [Queries and Mutations](../core/queries-and-mutations.md#mutation-decorator)

### @success / @failure / @result

**Purpose**: Helper decorators for mutation result types

**Usage**:
```python
from fraiseql.mutations.decorators import success, failure, result

@success
class CreateUserSuccess:
    user: User
    message: str

@failure
class CreateUserError:
    code: str
    message: str
    field: str | None = None

@result
class CreateUserResult:
    success: CreateUserSuccess | None = None
    error: CreateUserError | None = None
```

**Note**: These are type markers, not required for mutations. Use @type instead for most cases.

## Field Decorators

### @field

**Purpose**: Mark methods as GraphQL fields with custom resolvers

**Signature**:
```python
@field(
    resolver: Callable[..., Any] | None = None,
    description: str | None = None,
    track_n1: bool = True
)
def method_name(self, info, ...params) -> ReturnType:
    pass
```

**Parameters**:

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| method | Callable | - | Method to decorate (when used without parentheses) |
| resolver | Callable \| None | None | Optional custom resolver function |
| description | str \| None | None | Field description for GraphQL schema |
| track_n1 | bool | True | Track N+1 query patterns for performance monitoring |

**Examples**:
```python
@type
class User:
    first_name: str
    last_name: str

    @field(description="Full display name")
    def display_name(self) -> str:
        return f"{self.first_name} {self.last_name}"

    @field(description="User's posts")
    async def posts(self, info) -> list[Post]:
        db = info.context["db"]
        return await db.find("v_post", where={"user_id": self.id})

    @field(description="Posts with parameters")
    async def recent_posts(
        self,
        info,
        limit: int = 10
    ) -> list[Post]:
        db = info.context["db"]
        return await db.find(
            "v_post",
            where={"user_id": self.id},
            order_by="created_at DESC",
            limit=limit
        )
```

**See Also**: [Queries and Mutations](../core/queries-and-mutations.md#field-decorator)

### @dataloader_field

**Purpose**: Automatically use DataLoader for field resolution

**Signature**:
```python
@dataloader_field(
    loader_class: type[DataLoader],
    key_field: str,
    description: str | None = None
)
async def method_name(self, info) -> ReturnType:
    pass  # Implementation is auto-generated
```

**Parameters**:

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| loader_class | type[DataLoader] | Yes | DataLoader class to use for loading |
| key_field | str | Yes | Field name on parent object containing the key to load |
| description | str \| None | No | Field description for GraphQL schema |

**Examples**:
```python
from fraiseql import dataloader_field
from fraiseql.optimization.dataloader import DataLoader

# Define DataLoader
class UserDataLoader(DataLoader):
    async def batch_load(self, keys: list[UUID]) -> list[User | None]:
        db = self.context["db"]
        users = await db.find("v_user", where={"id__in": keys})
        # Return in same order as keys
        user_map = {user.id: user for user in users}
        return [user_map.get(key) for key in keys]

# Use in type
@type
class Post:
    author_id: UUID

    @dataloader_field(UserDataLoader, key_field="author_id")
    async def author(self, info) -> User | None:
        """Load post author using DataLoader."""
        pass  # Implementation is auto-generated

# GraphQL query automatically batches author loads
# query {
#   posts {
#     title
#     author { name }  # Batched into single query
#   }
# }
```

**Benefits**:
- Eliminates N+1 query problems
- Automatic batching of requests
- Built-in caching within single request
- Type-safe implementation

**See Also**: Optimization documentation

## Subscription Decorators

### @subscription

**Purpose**: Mark async generator functions as GraphQL subscriptions

**Signature**:
```python
@subscription
async def subscription_name(info, ...params) -> AsyncGenerator[ReturnType, None]:
    async for item in event_stream():
        yield item
```

**Parameters**: None

**Return Type**: Must be AsyncGenerator[YieldType, None]

**Examples**:
```python
from typing import AsyncGenerator

@subscription
async def on_post_created(info) -> AsyncGenerator[Post, None]:
    async for post in post_event_stream():
        yield post

@subscription
async def on_user_posts(
    info,
    user_id: UUID
) -> AsyncGenerator[Post, None]:
    async for post in post_event_stream():
        if post.user_id == user_id:
            yield post
```

**See Also**: [Queries and Mutations](../core/queries-and-mutations.md#subscription-decorator)

## Authentication Decorators

### @requires_auth

**Purpose**: Require authentication for resolver

**Signature**:
```python
@requires_auth
async def resolver_name(info, ...params) -> ReturnType:
    pass
```

**Parameters**: None

**Examples**:
```python
from fraiseql.auth import requires_auth

@query
@requires_auth
async def get_my_profile(info) -> User:
    user = info.context["user"]  # Guaranteed to be authenticated
    db = info.context["db"]
    return await db.find_one("v_user", where={"id": user.user_id})

@mutation
@requires_auth
async def update_profile(info, input: UpdateProfileInput) -> User:
    user = info.context["user"]
    db = info.context["db"]
    return await db.update_one(
        "v_user",
        where={"id": user.user_id},
        updates=input.__dict__
    )
```

**Raises**: GraphQLError with code "UNAUTHENTICATED" if not authenticated

### @requires_permission

**Purpose**: Require specific permission for resolver

**Signature**:
```python
@requires_permission(permission: str)
async def resolver_name(info, ...params) -> ReturnType:
    pass
```

**Parameters**:

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| permission | str | Yes | Permission string required (e.g., "users:write") |

**Examples**:
```python
from fraiseql.auth import requires_permission

@mutation
@requires_permission("users:write")
async def create_user(info, input: CreateUserInput) -> User:
    db = info.context["db"]
    return await db.create_one("v_user", data=input.__dict__)

@mutation
@requires_permission("users:delete")
async def delete_user(info, id: UUID) -> bool:
    db = info.context["db"]
    await db.delete_one("v_user", where={"id": id})
    return True
```

**Raises**:
- GraphQLError with code "UNAUTHENTICATED" if not authenticated
- GraphQLError with code "FORBIDDEN" if missing permission

### @requires_role

**Purpose**: Require specific role for resolver

**Signature**:
```python
@requires_role(role: str)
async def resolver_name(info, ...params) -> ReturnType:
    pass
```

**Parameters**:

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| role | str | Yes | Role name required (e.g., "admin") |

**Examples**:
```python
from fraiseql.auth import requires_role

@query
@requires_role("admin")
async def get_all_users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("v_user")

@mutation
@requires_role("admin")
async def admin_action(info, input: AdminActionInput) -> Result:
    # Admin-only mutation
    pass
```

**Raises**:
- GraphQLError with code "UNAUTHENTICATED" if not authenticated
- GraphQLError with code "FORBIDDEN" if missing role

### @requires_any_permission

**Purpose**: Require any of the specified permissions

**Signature**:
```python
@requires_any_permission(*permissions: str)
async def resolver_name(info, ...params) -> ReturnType:
    pass
```

**Parameters**:

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| *permissions | str | Yes | Variable number of permission strings |

**Examples**:
```python
from fraiseql.auth import requires_any_permission

@mutation
@requires_any_permission("users:write", "admin:all")
async def update_user(info, id: UUID, input: UpdateUserInput) -> User:
    # Can be performed by users:write OR admin:all
    db = info.context["db"]
    return await db.update_one("v_user", where={"id": id}, updates=input.__dict__)
```

**Raises**:
- GraphQLError with code "UNAUTHENTICATED" if not authenticated
- GraphQLError with code "FORBIDDEN" if missing all permissions

### @requires_any_role

**Purpose**: Require any of the specified roles

**Signature**:
```python
@requires_any_role(*roles: str)
async def resolver_name(info, ...params) -> ReturnType:
    pass
```

**Parameters**:

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| *roles | str | Yes | Variable number of role names |

**Examples**:
```python
from fraiseql.auth import requires_any_role

@query
@requires_any_role("admin", "moderator")
async def moderate_content(info, id: UUID) -> ModerationResult:
    # Can be performed by admin OR moderator
    pass
```

**Raises**:
- GraphQLError with code "UNAUTHENTICATED" if not authenticated
- GraphQLError with code "FORBIDDEN" if missing all roles

## Decorator Combinations

**Stacking decorators**:
```python
from fraiseql import query, connection, type
from fraiseql.auth import requires_auth, requires_permission
from fraiseql.types import Connection

# Multiple decorators - order matters
@connection(node_type=User)
@query
@requires_auth
@requires_permission("users:read")
async def users_connection(info, first: int | None = None) -> Connection[User]:
    pass

# Field-level auth
@type
class User:
    id: UUID
    name: str

    @field(description="Private settings")
    @requires_auth
    async def settings(self, info) -> UserSettings:
        # Only accessible to authenticated users
        pass
```

**Decorator Order Rules**:
1. Type decorators (@type, @input, @enum, @interface) - First
2. Query/Mutation/Subscription decorators - Second
3. Connection decorator - Before @query
4. Auth decorators - After query/mutation/field decorators
5. Field decorators (@field, @dataloader_field) - On methods

## See Also

- [Types and Schema](../core/types-and-schema.md) - Type system details
- [Queries and Mutations](../core/queries-and-mutations.md) - Query and mutation patterns
- [Configuration](../core/configuration.md) - Configure decorator behavior
