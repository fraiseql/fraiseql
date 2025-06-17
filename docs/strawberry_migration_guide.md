# Migration Guide: From Strawberry to FraiseQL

This guide helps you migrate your GraphQL API from Strawberry to FraiseQL, addressing common patterns and pitfalls.

## Table of Contents
1. [Key Differences](#key-differences)
2. [Type Definitions](#type-definitions)
3. [Query Patterns](#query-patterns)
4. [Mutations](#mutations)
5. [Context and Database Access](#context-and-database-access)
6. [Authentication](#authentication)
7. [Advanced Features](#advanced-features)
8. [Common Issues and Solutions](#common-issues-and-solutions)

## Key Differences

### Philosophy
- **Strawberry**: Python-first GraphQL library with decorators
- **FraiseQL**: PostgreSQL-integrated GraphQL with CQRS pattern

### Main Changes
1. FraiseQL uses `@fraise_type` instead of `@strawberry.type`
2. Queries can be simple functions (no class required)
3. Mutations follow a structured pattern with input/success/failure types
4. Built-in PostgreSQL integration with CQRS

## Type Definitions

### Basic Types

**Strawberry:**
```python
import strawberry
from typing import Optional

@strawberry.type
class User:
    id: strawberry.ID
    name: str
    email: str
    bio: Optional[str] = None
```

**FraiseQL:**
```python
from fraiseql import fraise_type
from typing import Optional
from uuid import UUID

@fraise_type
class User:
    id: UUID  # UUID is automatically converted to GraphQL ID
    name: str
    email: str
    bio: Optional[str] = None
```

### Input Types

**Strawberry:**
```python
@strawberry.input
class CreateUserInput:
    name: str
    email: str
```

**FraiseQL:**
```python
from fraiseql import fraise_input

@fraise_input
class CreateUserInput:
    name: str
    email: str
```

### Enums

**Strawberry:**
```python
@strawberry.enum
class UserRole(Enum):
    ADMIN = "admin"
    USER = "user"
```

**FraiseQL:**
```python
from fraiseql import fraise_enum

@fraise_enum
class UserRole(Enum):
    ADMIN = "admin"
    USER = "user"
```

### JSON Fields

**Strawberry:**
```python
@strawberry.type
class Config:
    settings: strawberry.scalars.JSON  # May not work
    # or
    settings: dict  # May cause issues
```

**FraiseQL:**
```python
from fraiseql import fraise_type
from typing import Any

@fraise_type
class Config:
    settings: dict[str, Any]  # Fully supported!
    # or use the JSON alias
    from fraiseql.types import JSON
    metadata: JSON
```

## Query Patterns

### Simple Queries

**Strawberry:**
```python
@strawberry.type
class Query:
    @strawberry.field
    async def user(self, id: strawberry.ID) -> Optional[User]:
        # Implementation
        pass

schema = strawberry.Schema(query=Query)
```

**FraiseQL (Option 1 - Direct Functions):**
```python
from fraiseql import query

@query
async def user(info, id: UUID) -> Optional[User]:
    db = info.context["db"]
    return await db.get_user(id)

# No need to create a Query class!
app = create_fraiseql_app(
    types=[User],
    # queries are auto-registered via @query decorator
)
```

**FraiseQL (Option 2 - QueryRoot Pattern):**
```python
from fraiseql import fraise_type, field

@fraise_type
class QueryRoot:
    @field
    async def user(self, root, info, id: UUID) -> Optional[User]:
        db = info.context["db"]
        return await db.get_user(id)
```

### Context Access

**Strawberry:**
```python
@strawberry.field
async def me(self, info: Info) -> User:
    request = info.context["request"]
    db = info.context["db"]
    # ...
```

**FraiseQL:**
```python
async def me(info) -> User:
    db = info.context["db"]  # Database is always available
    user = info.context["user"]  # User context if authenticated
    # ...
```

## Mutations

### Mutation Pattern

**Strawberry:**
```python
@strawberry.type
class Mutation:
    @strawberry.mutation
    async def create_user(
        self,
        name: str,
        email: str
    ) -> Union[User, UserError]:
        # Implementation
        pass
```

**FraiseQL:**
```python
from fraiseql import mutation, fraise_input, success, failure

@fraise_input
class CreateUserInput:
    name: str
    email: str

@success
@fraise_type
class CreateUserSuccess:
    user: User
    message: str = "User created successfully"

@failure
@fraise_type
class CreateUserFailure:
    code: str
    message: str

@mutation
class CreateUser:
    input: CreateUserInput
    success: CreateUserSuccess
    failure: CreateUserFailure  # Note: 'failure' not 'error'

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

## Context and Database Access

### Custom Context

**Strawberry:**
```python
async def get_context(request: Request) -> dict:
    return {
        "request": request,
        "db": database_connection,
        "user": await get_current_user(request)
    }

app = GraphQL(schema, context_getter=get_context)
```

**FraiseQL:**
```python
async def custom_context_getter(request: Request) -> dict[str, Any]:
    return {
        "db": database_connection,  # Or use built-in
        "user": await get_current_user(request),
        "custom_data": {"key": "value"}
    }

app = create_fraiseql_app(
    context_getter=custom_context_getter,
    # ...
)
```

### Lifespan Management

**Strawberry (with FastAPI):**
```python
@asynccontextmanager
async def lifespan(app: FastAPI):
    # Startup
    pool = AsyncConnectionPool(...)
    await pool.open()
    app.state.pool = pool
    yield
    # Shutdown
    await pool.close()

app = FastAPI(lifespan=lifespan)
```

**FraiseQL:**
```python
@asynccontextmanager
async def custom_lifespan(app: FastAPI):
    # Your custom initialization
    app.state.my_resource = await create_resource()
    yield
    # Your custom cleanup
    await app.state.my_resource.close()

app = create_fraiseql_app(
    lifespan=custom_lifespan,  # FraiseQL handles DB pool automatically
    # ...
)
```

## Authentication

### Decorators

**Strawberry:**
```python
from strawberry.permission import BasePermission

class IsAuthenticated(BasePermission):
    message = "User is not authenticated"

    def has_permission(self, source, info, **kwargs):
        return info.context["user"] is not None

@strawberry.field(permission_classes=[IsAuthenticated])
async def protected_field(self) -> str:
    return "secret"
```

**FraiseQL:**
```python
from fraiseql import requires_auth

@requires_auth
async def protected_query(info) -> str:
    return "secret"

# Or with permissions
from fraiseql import requires_permission

@requires_permission("users:read")
async def get_users(info) -> list[User]:
    # ...
```

## Advanced Features

### Interfaces

**Strawberry:**
```python
@strawberry.interface
class Node:
    id: strawberry.ID

@strawberry.type
class User(Node):
    name: str
```

**FraiseQL:**
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

### Field Resolvers

**Strawberry:**
```python
@strawberry.type
class User:
    id: strawberry.ID

    @strawberry.field
    async def posts(self) -> list[Post]:
        return await fetch_user_posts(self.id)
```

**FraiseQL:**
```python
# Option 1: Use CQRS pattern with PostgreSQL views
# Option 2: Use field decorator
@fraise_type
class QueryRoot:
    @field
    async def user_posts(self, root, info, user_id: UUID) -> list[Post]:
        db = info.context["db"]
        return await db.get_user_posts(user_id)
```

## Common Issues and Solutions

### 1. Environment Variables

**Issue**: Validation errors from environment variables like `ENV`, `DEBUG`

**Solution**: FraiseQL uses `FRAISEQL_` prefix for all env vars:
```bash
# Instead of:
DATABASE_URL=postgresql://...

# Use:
FRAISEQL_DATABASE_URL=postgresql://...
```

### 2. Database URL Format

**Issue**: Using psycopg2 connection strings

**Solution**: FraiseQL now auto-converts both formats:
```python
# Both work:
database_url="postgresql://user:pass@host/db"
database_url="dbname='db' user='user' host='host'"
```

### 3. Query Registration

**Issue**: "Type Query must define one or more fields"

**Solution**: Use the `@query` decorator or pass functions to `queries` parameter:
```python
# Option 1: Decorator
@query
async def get_user(info, id: UUID) -> User:
    pass

# Option 2: Direct registration
app = create_fraiseql_app(
    queries=[get_user, get_posts],
)
```

### 4. Missing Decorators

**Issue**: `@fraiseql.query` not found

**Solution**: Import from top-level:
```python
from fraiseql import query, field, mutation
```

## Complete Migration Example

Here's a complete before/after example:

**Strawberry:**
```python
import strawberry
from typing import Optional, List

@strawberry.type
class User:
    id: strawberry.ID
    name: str
    email: str

@strawberry.input
class CreateUserInput:
    name: str
    email: str

@strawberry.type
class UserError:
    message: str

@strawberry.type
class Query:
    @strawberry.field
    async def users(self) -> List[User]:
        # Fetch from DB
        pass

    @strawberry.field
    async def user(self, id: strawberry.ID) -> Optional[User]:
        # Fetch from DB
        pass

@strawberry.type
class Mutation:
    @strawberry.mutation
    async def create_user(
        self,
        input: CreateUserInput
    ) -> Union[User, UserError]:
        # Create user
        pass

schema = strawberry.Schema(query=Query, mutation=Mutation)
```

**FraiseQL:**
```python
from fraiseql import (
    fraise_type, fraise_input, query, mutation,
    success, failure, create_fraiseql_app
)
from uuid import UUID
from typing import Optional

@fraise_type
class User:
    id: UUID
    name: str
    email: str

@fraise_input
class CreateUserInput:
    name: str
    email: str

@success
@fraise_type
class CreateUserSuccess:
    user: User

@failure
@fraise_type
class CreateUserFailure:
    message: str

# Queries as simple functions
@query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.get_all_users()

@query
async def user(info, id: UUID) -> Optional[User]:
    db = info.context["db"]
    return await db.get_user(id)

# Mutation with pattern
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
        except Exception as e:
            return CreateUserFailure(message=str(e))

# Create app - much simpler!
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User],
    # Queries and mutations auto-registered via decorators
)
```

## Getting Help

If you encounter issues during migration:

1. Check the [FraiseQL documentation](https://github.com/fraiseql/fraiseql)
2. Review the error messages - they often suggest the solution
3. File an issue with a minimal reproduction example

Remember: FraiseQL is designed to make GraphQL + PostgreSQL development simpler and more type-safe. The migration might require some adjustments, but the end result is cleaner, more maintainable code.
