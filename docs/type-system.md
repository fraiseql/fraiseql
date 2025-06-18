# Type System

FraiseQL provides a comprehensive type system that maps Python types to GraphQL types seamlessly. This document covers all available types and how to use them effectively.

## Scalar Types

### Built-in Scalars

FraiseQL supports all GraphQL built-in scalars:

```python
@fraiseql.type
class User:
    id: int           # GraphQL Int
    name: str         # GraphQL String
    balance: float    # GraphQL Float
    is_active: bool   # GraphQL Boolean
```

### Custom Scalars

Define custom scalar types for special data:

```python
from datetime import datetime, date
from decimal import Decimal
from uuid import UUID

@fraiseql.type
class Event:
    id: UUID          # Custom UUID scalar
    name: str
    start_date: date  # Custom Date scalar
    created_at: datetime  # Custom DateTime scalar
    price: Decimal    # Custom Decimal scalar
```

### JSON Scalar

Handle arbitrary JSON data:

```python
from fraiseql import JSON

@fraiseql.type
class Settings:
    id: int
    preferences: JSON  # Any JSON data
    metadata: dict     # Also maps to JSON
```

## Object Types

### Basic Object Types

Define GraphQL object types with the `@type` decorator:

```python
@fraiseql.type
class User:
    id: int
    name: str
    email: str
```

### Nested Objects

Objects can contain other objects:

```python
@fraiseql.type
class Address:
    street: str
    city: str
    country: str

@fraiseql.type
class User:
    id: int
    name: str
    address: Address  # Nested object
```

### Optional Fields

Use union types for optional fields:

```python
@fraiseql.type
class User:
    id: int
    name: str
    avatar_url: str | None  # Optional field
    profile: 'UserProfile | None'  # Optional with forward reference
```

## List Types

### Simple Lists

Define lists of scalars or objects:

```python
@fraiseql.type
class User:
    id: int
    tags: list[str]        # List of strings
    scores: list[int]      # List of integers
    posts: list['Post']    # List of objects
```

### Non-nullable Lists

Control nullability at different levels:

```python
@fraiseql.type
class User:
    # Non-nullable list of non-nullable strings
    tags: list[str]
    
    # Nullable list of non-nullable strings
    optional_tags: list[str] | None
    
    # Non-nullable list of nullable strings
    maybe_tags: list[str | None]
    
    # Nullable list of nullable strings
    all_optional: list[str | None] | None
```

## Input Types

### Basic Input Types

Define input types for mutations:

```python
@fraiseql.input
class CreateUserInput:
    name: str
    email: str
    age: int | None = None  # Optional with default
```

### Nested Input Types

Input types can contain other input types:

```python
@fraiseql.input
class AddressInput:
    street: str
    city: str
    country: str

@fraiseql.input
class CreateUserInput:
    name: str
    email: str
    address: AddressInput  # Nested input
```

### Input Validation

Add validation to input types:

```python
from pydantic import validator

@fraiseql.input
class CreateUserInput:
    name: str
    email: str
    age: int
    
    @validator('email')
    def validate_email(cls, v):
        if '@' not in v:
            raise ValueError('Invalid email format')
        return v
    
    @validator('age')
    def validate_age(cls, v):
        if v < 0 or v > 150:
            raise ValueError('Age must be between 0 and 150')
        return v
```

## Enum Types

### Basic Enums

Define enumeration types:

```python
from enum import Enum

@fraiseql.enum
class UserRole(Enum):
    ADMIN = "admin"
    USER = "user"
    MODERATOR = "moderator"

@fraiseql.type
class User:
    id: int
    name: str
    role: UserRole
```

### String Enums

Enums with string values:

```python
@fraiseql.enum
class Status(Enum):
    ACTIVE = "active"
    INACTIVE = "inactive"
    PENDING = "pending"
```

### Integer Enums

Enums with integer values:

```python
@fraiseql.enum
class Priority(Enum):
    LOW = 1
    MEDIUM = 2
    HIGH = 3
```

## Interface Types

### Basic Interfaces

Define shared fields across types:

```python
@fraiseql.interface
class Node:
    id: int

@fraiseql.type
class User(Node):
    name: str
    email: str

@fraiseql.type
class Post(Node):
    title: str
    content: str
```

### Multiple Interfaces

Types can implement multiple interfaces:

```python
@fraiseql.interface
class Node:
    id: int

@fraiseql.interface
class Timestamped:
    created_at: datetime
    updated_at: datetime

@fraiseql.type
class Post(Node, Timestamped):
    title: str
    content: str
```

## Union Types

### Basic Unions

Define types that can be one of several types:

```python
from typing import Union

@fraiseql.type
class User:
    id: int
    name: str

@fraiseql.type
class Bot:
    id: int
    name: str
    version: str

# Union type
Actor = Union[User, Bot]

@fraiseql.type
class Message:
    id: int
    content: str
    sender: Actor  # Can be User or Bot
```

### Union Resolvers

Handle union type resolution:

```python
@fraiseql.union
class SearchResult:
    types = [User, Post, Comment]
    
    @staticmethod
    def resolve_type(obj, info, type_):
        if hasattr(obj, 'email'):
            return User
        elif hasattr(obj, 'title'):
            return Post
        else:
            return Comment
```

## Generic Types

### Connection Types

Use built-in connection types for pagination:

```python
from fraiseql import Connection, Edge

@fraiseql.type
class UserConnection(Connection['User']):
    pass

@fraiseql.type
class UserEdge(Edge['User']):
    pass

@fraiseql.query
async def users(info, first: int = 10) -> UserConnection:
    return await paginate_users(first=first)
```

### Custom Generic Types

Define your own generic types:

```python
from typing import TypeVar, Generic

T = TypeVar('T')

@fraiseql.type
class Result(Generic[T]):
    success: bool
    data: T | None = None
    error: str | None = None

# Usage
@fraiseql.mutation
async def create_user(info, input: CreateUserInput) -> Result[User]:
    try:
        user = await User.create(**input.dict())
        return Result(success=True, data=user)
    except Exception as e:
        return Result(success=False, error=str(e))
```

## Field Types

### Simple Fields

Basic field definitions:

```python
@fraiseql.type
class User:
    id: int
    name: str = fraiseql.field(description="User's display name")
    email: str = fraiseql.field(description="User's email address")
```

### Computed Fields

Fields with custom resolvers:

```python
@fraiseql.type
class User:
    first_name: str
    last_name: str
    
    @fraiseql.field
    def full_name(self, info) -> str:
        return f"{self.first_name} {self.last_name}"
```

### Async Fields

Fields that perform async operations:

```python
@fraiseql.type
class User:
    id: int
    
    @fraiseql.field
    async def posts(self, info) -> list['Post']:
        return await Post.get_by_user_id(self.id)
```

### Fields with Arguments

Fields that accept arguments:

```python
@fraiseql.type
class User:
    id: int
    
    @fraiseql.field
    def posts(self, info, limit: int = 10, offset: int = 0) -> list['Post']:
        return Post.get_by_user_id(self.id, limit=limit, offset=offset)
```

## Type Annotations

### Forward References

Use string annotations for forward references:

```python
@fraiseql.type
class User:
    id: int
    posts: list['Post']  # Forward reference

@fraiseql.type
class Post:
    id: int
    author: User  # Now defined
```

### Type Aliases

Create reusable type aliases:

```python
from typing import TypeAlias

ID: TypeAlias = int
Timestamp: TypeAlias = datetime

@fraiseql.type
class User:
    id: ID
    created_at: Timestamp
```

## Validation and Constraints

### Field Validation

Add validation to individual fields:

```python
@fraiseql.type
class User:
    name: str = fraiseql.field(
        description="User name",
        validators=[lambda x: len(x) >= 2]
    )
```

### Type-level Validation

Add validation to entire types:

```python
from pydantic import BaseModel, validator

@fraiseql.type
class User(BaseModel):
    name: str
    email: str
    age: int
    
    @validator('email')
    def validate_email(cls, v):
        if '@' not in v:
            raise ValueError('Invalid email')
        return v
```

## Best Practices

1. **Use meaningful type names**: Choose descriptive names for your types
2. **Keep types focused**: Each type should represent a single concept
3. **Use interfaces for shared fields**: Don't repeat common fields
4. **Handle nullability carefully**: Be explicit about what can be null
5. **Add descriptions**: Document your types and fields
6. **Validate input**: Always validate user input
7. **Use appropriate scalars**: Choose the right scalar type for your data
8. **Consider performance**: Use appropriate field resolvers and pagination

## Common Patterns

### Result Types

Handle success/error cases:

```python
@fraiseql.type
class CreateUserSuccess:
    user: User

@fraiseql.type  
class CreateUserError:
    message: str
    code: str

CreateUserResult = Union[CreateUserSuccess, CreateUserError]
```

### Pagination

Use connection patterns for pagination:

```python
@fraiseql.type
class PageInfo:
    has_next_page: bool
    has_previous_page: bool
    start_cursor: str | None
    end_cursor: str | None

@fraiseql.type
class UserEdge:
    node: User
    cursor: str

@fraiseql.type
class UserConnection:
    edges: list[UserEdge]
    page_info: PageInfo
    total_count: int
```

### Filtering

Define input types for filtering:

```python
@fraiseql.input
class UserFilter:
    name_contains: str | None = None
    email_contains: str | None = None
    role: UserRole | None = None
    is_active: bool | None = None

@fraiseql.query
async def users(info, filter: UserFilter | None = None) -> list[User]:
    return await User.filter(filter)
```