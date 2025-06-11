# Decorators

FraiseQL provides decorators to define GraphQL types, inputs, enums, and result unions.

## @fraiseql.type

Define GraphQL object types that map to database views.

```python
@fraiseql.type
class User:
    """A user in the system."""
    id: UUID
    name: str = fraise_field(description="User's display name")
    email: str = fraise_field(description="Email address")
    is_active: bool = fraise_field(default=True)
    created_at: datetime
```

### Parameters

- **table** (optional): Database table/view name. Defaults to `v_{plural_class_name}`
- **description** (optional): GraphQL type description. Defaults to class docstring

```python
@fraiseql.type(table="custom_users_view", description="Custom user type")
class User:
    pass
```

### Requirements

- Class must have type hints for all fields
- Corresponding database view must exist with matching `data` JSONB column
- View naming convention: `v_users` for `User` type

## @fraiseql.input

Define GraphQL input types for mutations and filters.

```python
@fraiseql.input
class CreateUserInput:
    """Input for creating a new user."""
    name: str
    email: str
    password: str
    bio: Optional[str] = None
```

### Parameters

- **description** (optional): GraphQL input description. Defaults to class docstring

### Validation

Input types support automatic validation:
- Required fields must be provided
- Optional fields use Python default values
- Type coercion happens automatically

## @fraiseql.enum

Define GraphQL enum types from Python enums.

```python
from enum import Enum

@fraiseql.enum
class UserRole(Enum):
    """User role levels."""
    ADMIN = "admin"
    MODERATOR = "moderator"
    USER = "user"
    GUEST = "guest"

# Usage in types
@fraiseql.type
class User:
    id: UUID
    role: UserRole = fraise_field(default=UserRole.USER)
```

### Requirements

- Must inherit from Python's `Enum` class
- Enum values should be strings for GraphQL compatibility

## @fraiseql.interface

Define GraphQL interface types for shared fields.

```python
@fraiseql.interface
class Node:
    """An object with a unique identifier."""
    id: UUID

@fraiseql.type
class User(Node):
    """User implements Node interface."""
    name: str
    email: str

@fraiseql.type
class Post(Node):
    """Post implements Node interface."""
    title: str
    content: str
```

### Interface Resolution

FraiseQL automatically handles interface type resolution based on the object's class name.

## Mutation Decorators

FraiseQL uses a PostgreSQL function-based approach for mutations where business logic lives in the database.

### @fraiseql.mutation

Define a mutation that calls a PostgreSQL function:

```python
@fraiseql.mutation
class CreateUser:
    """Create a new user account."""
    input: CreateUserInput
    success: CreateUserSuccess
    error: CreateUserError
```

The decorator:
- Derives the PostgreSQL function name from the class name (`CreateUser` → `graphql.create_user`)
- Generates a GraphQL resolver that calls the function
- Parses the standardized result into Success or Error types

### @fraiseql.success

Define successful result types:

```python
@fraiseql.success
class CreateUserSuccess:
    """Successful user creation."""
    message: str
    user: User  # Automatically instantiated from function result
```

### @fraiseql.failure

Define error result types:

```python
@fraiseql.failure
class CreateUserError:
    """Failed user creation."""
    message: str
    conflict_user: Optional[User] = None  # Can include complex objects
    suggested_email: Optional[str] = None
```

### Complete Example

```python
# 1. Define input
@fraiseql.input
class CreateUserInput:
    email: str
    name: str
    bio: Optional[str] = None

# 2. Define success type
@fraiseql.success
class CreateUserSuccess:
    message: str
    user: User

# 3. Define error type
@fraiseql.failure
class CreateUserError:
    message: str
    conflict_user: Optional[User] = None

# 4. Define mutation
@fraiseql.mutation
class CreateUser:
    """Create a new user account."""
    input: CreateUserInput
    success: CreateUserSuccess
    error: CreateUserError
```

The corresponding PostgreSQL function should return a `mutation_result` type. See [PostgreSQL Function-Based Mutations](../mutations/postgresql-function-based.md) for details.

### GraphQL Output

This generates the following GraphQL schema:

```graphql
union CreateUserResult = CreateUserSuccess | CreateUserError

type CreateUserSuccess {
  user: User!
  message: String!
}

type CreateUserError {
  message: String!
  code: String!
  fieldErrors: JSON
}

type Mutation {
  createUser(input: CreateUserInput!): CreateUserResult!
}
```

### Querying Results

Clients use inline fragments to handle different result types:

```graphql
mutation CreateUser($input: CreateUserInput!) {
  createUser(input: $input) {
    ... on CreateUserSuccess {
      user {
        id
        name
        email
      }
      message
    }
    ... on CreateUserError {
      message
      code
      fieldErrors
    }
  }
}
```

## Field and Mutation Decorators

### @fraiseql.field

Mark methods as GraphQL fields:

```python
@fraiseql.type
class Query:
    @fraiseql.field
    async def user(self, id: UUID, info: fraiseql.Info) -> Optional[User]:
        """Get a user by ID."""
        # Implementation
        pass

    @fraiseql.field
    async def users(
        self,
        info: fraiseql.Info,
        limit: int = 20,
        offset: int = 0
    ) -> list[User]:
        """Get users with pagination."""
        # Implementation
        pass
```

### @fraiseql.mutation

Alias for `@fraiseql.field` on mutation types:

```python
@fraiseql.type
class Mutation:
    @fraiseql.mutation  # Same as @fraiseql.field
    async def create_user(
        self,
        input: CreateUserInput,
        info: fraiseql.Info
    ) -> CreateUserResult:
        """Create a new user."""
        # Implementation
        pass
```

## Authentication Decorators

### @requires_auth

Require authentication for field access:

```python
from fraiseql.auth import requires_auth

@fraiseql.type
class Query:
    @fraiseql.field
    @requires_auth
    async def me(self, info: fraiseql.Info) -> User:
        """Get current authenticated user."""
        user_context = info.context["user"]
        # Implementation
        pass
```

### @requires_role

Require specific user roles:

```python
from fraiseql.auth import requires_role

@fraiseql.type
class Mutation:
    @fraiseql.field
    @requires_role("admin")
    async def delete_user(self, user_id: UUID) -> bool:
        """Delete a user (admin only)."""
        # Implementation
        pass
```

### @requires_permission

Require specific permissions:

```python
from fraiseql.auth import requires_permission

@fraiseql.type
class Mutation:
    @fraiseql.field
    @requires_permission("posts:write")
    async def create_post(self, input: CreatePostInput) -> Post:
        """Create a post (requires posts:write permission)."""
        # Implementation
        pass
```

## Best Practices

1. **Use descriptive docstrings** - They become GraphQL descriptions
2. **Consistent naming** - Follow Python conventions, FraiseQL handles GraphQL conversion
3. **Type hints everywhere** - Required for schema generation
4. **Use result unions** - Better error handling than exceptions
5. **Keep inputs focused** - Create specific input types for different operations
6. **Document enum values** - Use descriptive enum member names

## Error Handling

Decorators will raise `TypeError` if:
- Type hints are missing
- Invalid types are used
- Required database views don't exist
- Enum types aren't properly decorated

```python
# This will raise TypeError - missing type hints
@fraiseql.type
class BadType:
    name = "no type hint"  # ❌ Missing type hint

# This is correct
@fraiseql.type
class GoodType:
    name: str  # ✅ Has type hint
```
