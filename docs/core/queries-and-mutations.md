# Queries and Mutations

Decorators and patterns for defining GraphQL queries, mutations, and subscriptions.

**📍 Navigation**: [← Types & Schema](types-and-schema.md) • [Database API →](database-api.md) • [Performance →](../performance/index.md)

## @query Decorator

**Purpose**: Mark async functions as GraphQL queries

**Signature**:
```python
from fraiseql import type, query, mutation, input, field

@query
async def query_name(info, param1: Type1, param2: Type2 = default) -> ReturnType:
    pass
```

**Parameters**:

| Parameter | Required | Description |
|-----------|----------|-------------|
| info | Yes | GraphQL resolver info (first parameter) |
| ... | Varies | Query parameters with type annotations |

**Returns**: Any GraphQL type (fraise_type, list, scalar)

**Examples**:

Basic query with database access:
```python
from fraiseql import query, type
from uuid import UUID

@query
async def get_user(info, id: UUID) -> User:
    repo = info.context["repo"]
    # Returns RustResponseBytes - automatically processed by exclusive Rust pipeline
    return await repo.find_one_rust("v_user", "user", info, id=id)
```

Query with multiple parameters:
```python
from fraiseql import type, query, mutation, input, field

@query
async def search_users(
    info,
    name_filter: str | None = None,
    limit: int = 10
) -> list[User]:
    repo = info.context["repo"]
    filters = {}
    if name_filter:
        filters["name__icontains"] = name_filter
    # Exclusive Rust pipeline handles camelCase conversion and __typename injection
    return await repo.find_rust("v_user", "users", info, **filters, limit=limit)
```

Query with authentication:
```python
from fraiseql import type, query, mutation, input, field

from graphql import GraphQLError

@query
async def get_my_profile(info) -> User:
    user_context = info.context.get("user")
    if not user_context:
        raise GraphQLError("Authentication required")

    repo = info.context["repo"]
    # Exclusive Rust pipeline works with authentication automatically
    return await repo.find_one_rust("v_user", "user", info, id=user_context.user_id)
```

Query with error handling:
```python
from fraiseql import type, query, mutation, input, field

import logging

logger = logging.getLogger(__name__)

@query
async def get_post(info, id: UUID) -> Post | None:
    try:
        repo = info.context["repo"]
        # Exclusive Rust pipeline handles JSON processing automatically
        return await repo.find_one_rust("v_post", "post", info, id=id)
    except Exception as e:
        logger.error(f"Failed to fetch post {id}: {e}")
        return None
```

Query using custom repository methods:
```python
from fraiseql import type, query, mutation, input, field

@query
async def get_user_stats(info, user_id: UUID) -> UserStats:
    repo = info.context["repo"]
    # Custom SQL query for complex aggregations
    # Exclusive Rust pipeline handles result processing automatically
    result = await repo.execute_raw(
        "SELECT count(*) as post_count FROM posts WHERE user_id = $1",
        user_id
    )
    return UserStats(post_count=result[0]["post_count"])
```

**Notes**:
- Functions decorated with @query are automatically discovered and registered
- The first parameter is always 'info' (GraphQL resolver info)
- Return type annotation is used for GraphQL schema generation
- Use async/await for database operations
- Access repository via `info.context["repo"]` (provides exclusive Rust pipeline integration)
- Access user context via `info.context["user"]` (if authentication enabled)
- Exclusive Rust pipeline automatically handles camelCase conversion and __typename injection

## @field Decorator

**Purpose**: Mark methods as GraphQL fields with optional custom resolvers

**Signature**:
```python
from fraiseql import type, query, mutation, input, field

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
| method | Callable | - | The method to decorate (when used without parentheses) |
| resolver | Callable \| None | None | Optional custom resolver function |
| description | str \| None | None | Field description for GraphQL schema |
| track_n1 | bool | True | Track N+1 query patterns for performance monitoring |

**Examples**:

Computed field with description:
```python
from fraiseql import type, query, mutation, input, field

@type
class User:
    first_name: str
    last_name: str

    @field(description="User's full display name")
    def display_name(self) -> str:
        return f"{self.first_name} {self.last_name}"
```

Async field with database access:
```python
from fraiseql import type, query, mutation, input, field

@type
class User:
    id: UUID

    @field(description="Posts authored by this user")
    async def posts(self, info) -> list[Post]:
        repo = info.context["repo"]
        return await repo.find_rust("v_post", "posts", info, user_id=self.id)
```

Field with custom resolver function:
```python
from fraiseql import type, query, mutation, input, field

async def fetch_user_posts_optimized(root, info):
    """Custom resolver with optimized batch loading."""
    db = info.context["db"]
    # Use DataLoader or batch loading here
    return await batch_load_posts([root.id])

@type
class User:
    id: UUID

    @field(
        resolver=fetch_user_posts_optimized,
        description="Posts with optimized loading"
    )
    async def posts(self) -> list[Post]:
        # This signature defines GraphQL schema
        # but fetch_user_posts_optimized handles actual resolution
        pass
```

Field with parameters:
```python
from fraiseql import type, query, mutation, input, field

@type
class User:
    id: UUID

    @field(description="User's posts with optional filtering")
    async def posts(
        self,
        info,
        published_only: bool = False,
        limit: int = 10
    ) -> list[Post]:
        repo = info.context["repo"]
        filters = {"user_id": self.id}
        if published_only:
            filters["status"] = "published"
        return await repo.find_rust("v_post", "posts", info, **filters, limit=limit)
```

Field with authentication/authorization:
```python
from fraiseql import type, query, mutation, input, field

@type
class User:
    id: UUID

    @field(description="Private user settings (owner only)")
    async def settings(self, info) -> UserSettings | None:
        user_context = info.context.get("user")
        if not user_context or user_context.user_id != self.id:
            return None  # Don't expose private data

        repo = info.context["repo"]
        return await repo.find_one_rust("v_user_settings", "settings", info, user_id=self.id)
```

Field with caching:
```python
from fraiseql import type, query, mutation, input, field

@type
class Post:
    id: UUID

    @field(description="Number of likes (cached)")
    async def like_count(self, info) -> int:
        cache = info.context.get("cache")
        cache_key = f"post:{self.id}:likes"

        # Try cache first
        if cache:
            cached_count = await cache.get(cache_key)
            if cached_count is not None:
                return int(cached_count)

        # Fallback to database
        repo = info.context["repo"]
        result = await repo.execute_raw(
            "SELECT count(*) FROM likes WHERE post_id = $1",
            self.id
        )
        count = result[0]["count"]

        # Cache for 5 minutes
        if cache:
            await cache.set(cache_key, count, ttl=300)

        return count
```

**Notes**:
- Fields are automatically included in GraphQL schema generation
- Use 'info' parameter to access GraphQL context (database, user, etc.)
- Async fields support database queries and external API calls
- Custom resolvers can implement optimized data loading patterns
- N+1 query detection is automatically enabled for performance monitoring
- Return None from fields to indicate null values in GraphQL
- Type annotations enable automatic GraphQL type generation

## @connection Decorator

**Purpose**: Create cursor-based pagination query resolvers following Relay specification

**Signature**:
```python
from fraiseql import type, query, mutation, input, field

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
@query
async def query_name(
    info,
    first: int | None = None,
    after: str | None = None,
    where: dict | None = None
) -> Connection[NodeType]:
    pass  # Implementation handled by decorator
```

**Parameters**:

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| node_type | type | Required | Type of objects in the connection |
| view_name | str \| None | None | Database view name (inferred from function name if omitted) |
| default_page_size | int | 20 | Default number of items per page |
| max_page_size | int | 100 | Maximum allowed page size |
| include_total_count | bool | True | Include total count in results |
| cursor_field | str | "id" | Field to use for cursor ordering |
| jsonb_extraction | bool \| None | None | Enable JSONB field extraction (inherits from global config if None) |
| jsonb_column | str \| None | None | JSONB column name (inherits from global config if None) |

**Returns**: Connection[T] with edges, page_info, and total_count

**Raises**: ValueError if configuration parameters are invalid

**Examples**:

Basic connection query:
```python
from fraiseql import connection, query, type
from fraiseql.types import Connection

@type(sql_source="v_user")
class User:
    id: UUID
    name: str
    email: str

@connection(node_type=User)
@query
async def users_connection(info, first: int | None = None) -> Connection[User]:
    pass  # Implementation handled by decorator
```

Connection with custom configuration:
```python
from fraiseql import type, query, mutation, input, field

@connection(
    node_type=Post,
    view_name="v_published_posts",
    default_page_size=25,
    max_page_size=50,
    cursor_field="created_at",
    jsonb_extraction=True,
    jsonb_column="data"
)
@query
async def posts_connection(
    info,
    first: int | None = None,
    after: str | None = None,
    where: dict[str, Any] | None = None
) -> Connection[Post]:
    pass
```

With filtering and ordering:
```python
from fraiseql import type, query, mutation, input, field

@connection(node_type=User, cursor_field="created_at")
@query
async def recent_users_connection(
    info,
    first: int | None = None,
    after: str | None = None,
    where: dict[str, Any] | None = None
) -> Connection[User]:
    pass
```

**GraphQL Usage**:
```graphql
query {
  usersConnection(first: 10, after: "cursor123") {
    edges {
      node {
        id
        name
        email
      }
      cursor
    }
    pageInfo {
      hasNextPage
      hasPreviousPage
      startCursor
      endCursor
      totalCount
    }
    totalCount
  }
}
```

**Notes**:
- Functions must be async and take 'info' as first parameter
- The decorator handles all pagination logic automatically
- Uses existing repository.paginate() method
- Returns properly typed Connection[T] objects
- Supports all Relay connection specification features
- View name is inferred from function name (e.g., users_connection → v_users)

## @mutation Decorator

**Purpose**: Define GraphQL mutations with PostgreSQL function backing

**Signature**:

Function-based mutation:
```python
from fraiseql import type, query, mutation, input, field

@mutation
async def mutation_name(info, input: InputType) -> ReturnType:
    pass
```

Class-based mutation:
```python
from fraiseql import type, query, mutation, input, field

@mutation(
    function: str | None = None,
    schema: str | None = None,
    context_params: dict[str, str] | None = None,
    error_config: MutationErrorConfig | None = None
)
class MutationName:
    input: InputType
    success: SuccessType
    failure: FailureType  # or error: ErrorType
```

**Parameters (Class-based)**:

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| function | str \| None | None | PostgreSQL function name (defaults to snake_case of class name) |
| schema | str \| None | "public" | PostgreSQL schema containing the function |
| context_params | dict[str, str] \| None | None | Maps GraphQL context keys to PostgreSQL function parameters |
| error_config | MutationErrorConfig \| None | None | Configuration for error detection behavior |

**Examples**:

Simple function-based mutation:
```python
from fraiseql import type, query, mutation, input, field

@mutation
async def create_user(info, input: CreateUserInput) -> User:
    db = info.context["db"]
    user_data = {
        "name": input.name,
        "email": input.email,
        "created_at": datetime.utcnow()
    }
    result = await db.execute_raw(
        "INSERT INTO users (data) VALUES ($1) RETURNING *",
        user_data
    )
    return User(**result[0]["data"])
```

Basic class-based mutation:
```python
from fraiseql import mutation, input, type

@input
class CreateUserInput:
    name: str
    email: str

@type
class CreateUserSuccess:
    user: User
    message: str

@type
class CreateUserError:
    code: str
    message: str
    field: str | None = None

@mutation
class CreateUser:
    input: CreateUserInput
    success: CreateUserSuccess
    failure: CreateUserError

# Automatically calls PostgreSQL function: public.create_user(input)
# and parses result into CreateUserSuccess or CreateUserError
```

Mutation with custom PostgreSQL function:
```python
from fraiseql import type, query, mutation, input, field

@mutation(function="register_new_user", schema="auth")
class RegisterUser:
    input: RegistrationInput
    success: RegistrationSuccess
    failure: RegistrationError

# Calls: auth.register_new_user(input) instead of default name
```

Mutation with context parameters:
```python
from fraiseql import type, query, mutation, input, field

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

# Calls: app.create_location(tenant_id, user_id, input)
# Where tenant_id comes from info.context["tenant_id"]
# And user_id comes from info.context["user"].user_id
```

Mutation with validation:
```python
from fraiseql import type, query, mutation, input, field

@input
class UpdateUserInput:
    id: UUID
    name: str | None = None
    email: str | None = None

@mutation
async def update_user(info, input: UpdateUserInput) -> User:
    db = info.context["db"]
    user_context = info.context.get("user")

    # Authorization check
    if not user_context:
        raise GraphQLError("Authentication required")

    # Validation
    if input.email and not is_valid_email(input.email):
        raise GraphQLError("Invalid email format")

    # Update logic
    updates = {}
    if input.name:
        updates["name"] = input.name
    if input.email:
        updates["email"] = input.email

    if not updates:
        raise GraphQLError("No fields to update")

    return await db.update_one("v_user", where={"id": input.id}, updates=updates)
```

Multi-step mutation with transaction:
```python
from fraiseql import type, query, mutation, input, field

@mutation
async def transfer_funds(
    info,
    input: TransferInput
) -> TransferResult:
    db = info.context["db"]

    async with db.transaction():
        # Validate source account
        source = await db.find_one(
            "v_account",
            where={"id": input.source_account_id}
        )
        if not source or source.balance < input.amount:
            raise GraphQLError("Insufficient funds")

        # Validate destination account
        dest = await db.find_one(
            "v_account",
            where={"id": input.destination_account_id}
        )
        if not dest:
            raise GraphQLError("Destination account not found")

        # Perform transfer
        await db.update_one(
            "v_account",
            where={"id": source.id},
            updates={"balance": source.balance - input.amount}
        )
        await db.update_one(
            "v_account",
            where={"id": dest.id},
            updates={"balance": dest.balance + input.amount}
        )

        # Log transaction
        transfer = await db.create_one("v_transfer", data={
            "source_account_id": input.source_account_id,
            "destination_account_id": input.destination_account_id,
            "amount": input.amount,
            "created_at": datetime.utcnow()
        })

        return TransferResult(
            transfer=transfer,
            new_source_balance=source.balance - input.amount,
            new_dest_balance=dest.balance + input.amount
        )
```

Mutation with input transformation (prepare_input hook):
```python
from fraiseql import type, query, mutation, input, field

@input
class NetworkConfigInput:
    ip_address: str
    subnet_mask: str

@mutation
class CreateNetworkConfig:
    input: NetworkConfigInput
    success: NetworkConfigSuccess
    failure: NetworkConfigError

    @staticmethod
    def prepare_input(input_data: dict) -> dict:
        """Transform IP + subnet mask to CIDR notation."""
        ip = input_data.get("ip_address")
        mask = input_data.get("subnet_mask")

        if ip and mask:
            # Convert subnet mask to CIDR prefix
            cidr_prefix = {
                "255.255.255.0": 24,
                "255.255.0.0": 16,
                "255.0.0.0": 8,
            }.get(mask, 32)

            return {
                "ip_address": f"{ip}/{cidr_prefix}",
                # subnet_mask field is removed
            }
        return input_data

# Frontend sends: { ipAddress: "192.168.1.1", subnetMask: "255.255.255.0" }
# Database receives: { ip_address: "192.168.1.1/24" }
```

**PostgreSQL Function Requirements**:

For class-based mutations, the PostgreSQL function should:

1. Accept input as JSONB parameter
2. Return a result with 'success' boolean field
3. Include either 'data' field (success) or 'error' field (failure)

Example PostgreSQL function:
```sql
CREATE OR REPLACE FUNCTION public.create_user(input jsonb)
RETURNS jsonb
LANGUAGE plpgsql
AS $$
DECLARE
    user_id uuid;
    result jsonb;
BEGIN
    -- Insert user
    INSERT INTO users (name, email, created_at)
    VALUES (
        input->>'name',
        input->>'email',
        now()
    )
    RETURNING id INTO user_id;

    -- Return success response
    result := jsonb_build_object(
        'success', true,
        'data', jsonb_build_object(
            'id', user_id,
            'name', input->>'name',
            'email', input->>'email',
            'message', 'User created successfully'
        )
    );

    RETURN result;
EXCEPTION
    WHEN unique_violation THEN
        -- Return error response
        result := jsonb_build_object(
            'success', false,
            'error', jsonb_build_object(
                'code', 'EMAIL_EXISTS',
                'message', 'Email address already exists',
                'field', 'email'
            )
        );
        RETURN result;
END;
$$;
```

**Notes**:
- Function-based mutations provide full control over implementation
- Class-based mutations automatically integrate with PostgreSQL functions
- Use transactions for multi-step operations to ensure data consistency
- PostgreSQL functions handle validation and business logic at database level
- Context parameters enable tenant isolation and user tracking
- Success/error types provide structured response handling
- All mutations are automatically registered with GraphQL schema
- prepare_input hook allows transforming input data before database calls
- prepare_input is called after GraphQL validation but before PostgreSQL function

## @subscription Decorator

**Purpose**: Mark async generator functions as GraphQL subscriptions for real-time updates

**Signature**:
```python
@subscription
async def subscription_name(info, ...params) -> AsyncGenerator[ReturnType, None]:
    async for item in event_stream():
        yield item
```

**Examples**:

Basic subscription:
```python
from typing import AsyncGenerator

@subscription
async def on_post_created(info) -> AsyncGenerator[Post, None]:
    # Subscribe to post creation events
    async for post in post_event_stream():
        yield post
```

Filtered subscription with parameters:
```python
@subscription
async def on_user_posts(
    info,
    user_id: UUID
) -> AsyncGenerator[Post, None]:
    # Only yield posts from specific user
    async for post in post_event_stream():
        if post.user_id == user_id:
            yield post
```

Subscription with authentication:
```python
@subscription
async def on_private_messages(info) -> AsyncGenerator[Message, None]:
    user_context = info.context.get("user")
    if not user_context:
        raise GraphQLError("Authentication required")

    async for message in message_stream():
        # Only yield messages for authenticated user
        if message.recipient_id == user_context.user_id:
            yield message
```

Subscription with database polling:
```python
import asyncio

@subscription
async def on_task_updates(
    info,
    project_id: UUID
) -> AsyncGenerator[Task, None]:
    db = info.context["db"]
    last_check = datetime.utcnow()

    while True:
        # Poll for new/updated tasks
        updated_tasks = await db.find(
            "v_task",
            where={
                "project_id": project_id,
                "updated_at__gt": last_check
            }
        )

        for task in updated_tasks:
            yield task

        last_check = datetime.utcnow()
        await asyncio.sleep(1)  # Poll every second
```

**Notes**:
- Subscription functions MUST be async generators (use 'async def' and 'yield')
- Return type must be AsyncGenerator[YieldType, None]
- The first parameter is always 'info' (GraphQL resolver info)
- Use WebSocket transport for GraphQL subscriptions
- Consider rate limiting and authentication for production use
- Handle connection cleanup in finally blocks
- Use asyncio.sleep() for polling-based subscriptions

## See Also

- [Types and Schema](./types-and-schema.md) - Define types for use in queries and mutations
- [Decorators Reference](../reference/decorators.md) - Complete decorator API
- [Database API](../reference/database.md) - Database operations for queries and mutations
