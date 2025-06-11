# Type System

FraiseQL's type system bridges Python's type hints with GraphQL's schema definition language, providing a seamless development experience.

## Type Decorators

### @fraiseql.type

Defines a GraphQL object type:

```python
@fraiseql.type
class User:
    """A user in the system"""
    id: int
    name: str
    email: str
    created_at: datetime
```

### @fraiseql.input

Defines a GraphQL input type:

```python
@fraiseql.input
class CreateUserInput:
    name: str
    email: str
    password: str
```

### @fraiseql.interface

Defines a GraphQL interface:

```python
@fraiseql.interface
class Node:
    """An object with an ID"""
    id: int

@fraiseql.type(implements=[Node])
class User:
    id: int
    name: str
    email: str

@fraiseql.type(implements=[Node])
class Article:
    id: int
    title: str
    content: str
```

#### PostgreSQL Implementation

In PostgreSQL, interfaces are implemented using UNION ALL views that combine data from multiple tables. Each row includes a `__typename` field for type resolution:

```sql
-- Create a view for the Node interface
CREATE VIEW v_node AS
SELECT id, data || jsonb_build_object('__typename', 'User') as data
FROM users
UNION ALL
SELECT id, data || jsonb_build_object('__typename', 'Article') as data
FROM articles;

-- Query the interface view
SELECT data FROM v_node WHERE data->>'created_at' > '2024-01-01';
```

#### Using Interface Queries

Query interface views using the repository's `query_interface` method:

```python
# Get all nodes
nodes = await repo.query_interface("v_node")

# Get a specific node by ID (polymorphic)
user_or_article = await repo.get_polymorphic_by_id(
    "v_node",
    entity_id,
    type_mapping={"User": User, "Article": Article}
)

# Query with filters
recent_nodes = await repo.query_interface(
    "v_node",
    filters={"created_at": {"$gt": "2024-01-01"}},
    order_by="created_at DESC",
    limit=10
)
```

### @fraiseql.enum

Defines a GraphQL enum:

```python
@fraiseql.enum
class UserRole:
    ADMIN = "admin"
    USER = "user"
    GUEST = "guest"
```

## Field Definitions

### fraise_field()

The `fraise_field()` function adds metadata to fields:

```python
from fraiseql import fraise_field

@fraiseql.type
class Product:
    id: int
    name: str = fraise_field(
        description="Product display name",
        deprecation_reason="Use displayName instead"
    )
    display_name: str = fraise_field(
        description="Product display name (new)"
    )
    price: float = fraise_field(
        description="Price in USD",
        purpose="Display price to customers"
    )
    in_stock: bool = fraise_field(
        default=True,
        description="Whether product is available"
    )
```

### Field Parameters

- `description`: GraphQL field description
- `purpose`: Internal documentation (not exposed to GraphQL)
- `default`: Default value for the field
- `deprecation_reason`: Mark field as deprecated
- `permission`: Required permission to access field

## Type Mapping

FraiseQL automatically maps Python types to GraphQL:

| Python Type | GraphQL Type |
|------------|--------------|
| `str` | `String` |
| `int` | `Int` |
| `float` | `Float` |
| `bool` | `Boolean` |
| `list[T]` | `[T]` |
| `Optional[T]` | `T` (nullable) |
| `datetime` | `DateTime` (custom scalar) |
| `date` | `Date` (custom scalar) |
| `UUID` | `UUID` (custom scalar) |
| `dict` or `Any` | `JSON` (custom scalar) |

## Custom Scalars

FraiseQL includes several custom scalars:

### DateTime
```python
from datetime import datetime

@fraiseql.type
class Event:
    id: int
    name: str
    starts_at: datetime  # ISO 8601 format
```

### UUID
```python
from uuid import UUID

@fraiseql.type
class User:
    id: UUID
    name: str
```

### JSON
```python
from typing import Any

@fraiseql.type
class Settings:
    id: int
    preferences: dict[str, Any]  # Arbitrary JSON
```

### Email
```python
from fraiseql.types.scalars import EmailAddress

@fraiseql.type
class User:
    id: int
    email: EmailAddress  # Validates email format
```

## Relationships

Define relationships between types:

```python
@fraiseql.type
class User:
    id: int
    name: str
    posts: list["Post"] = fraise_field(
        description="Posts written by this user"
    )

@fraiseql.type
class Post:
    id: int
    title: str
    author: User = fraise_field(
        description="Post author"
    )
    comments: list["Comment"] = fraise_field(
        description="Comments on this post"
    )

@fraiseql.type
class Comment:
    id: int
    content: str
    author: User
    post: Post
```

## Optional and Nullable Fields

```python
from typing import Optional

@fraiseql.type
class User:
    id: int
    name: str
    bio: Optional[str] = None  # Nullable in GraphQL
    email: str  # Required in GraphQL
    avatar_url: Optional[str] = fraise_field(
        default=None,
        description="User's avatar URL"
    )
```

## Generic Types

FraiseQL supports generic types:

```python
from typing import Generic, TypeVar

T = TypeVar('T')

@fraiseql.type
class PaginatedResult(Generic[T]):
    items: list[T]
    total: int
    page: int
    page_size: int

# Usage
@fraiseql.type
class Query:
    @fraiseql.field
    async def users(self, page: int = 1) -> PaginatedResult[User]:
        # Implementation
        pass
```

## Union Types

Define GraphQL unions for result types:

```python
from fraiseql import result, success, failure

@result
class CreateUserResult:
    """Result of user creation"""

@success
class CreateUserSuccess:
    user: User
    message: str = "User created successfully"

@failure
class CreateUserError:
    code: str
    message: str
```

## Type Registration

Types are automatically registered when decorated, but you can also manually register:

```python
from fraiseql.gql import schema_builder

# Automatic registration
@fraiseql.type
class User:
    id: int
    name: str

# Manual registration (rarely needed)
schema_builder.register_type(User)
```

## Type Validation

FraiseQL validates types at multiple levels:

1. **Python Type Checking**: Use mypy or pyright
2. **Runtime Validation**: Automatic validation during execution
3. **GraphQL Schema**: Schema validation on startup

## Best Practices

1. **Use Type Hints**: Always provide type hints for better IDE support
2. **Document Fields**: Use `fraise_field()` with descriptions
3. **Nullable vs Required**: Be explicit about nullable fields
4. **Enums for Constants**: Use enums instead of string literals
5. **Interfaces for Shared Fields**: Define common fields in interfaces
6. **Interface Views**: Create UNION ALL views in PostgreSQL for each interface, including `__typename` for polymorphic queries
7. **Type Mapping**: Provide type mappings when querying interface views for proper type instantiation

## Next Steps

- Learn about [Database Views](./database-views.md)
- Understand [Query Translation](./query-translation.md)
- Review [API Reference](../api-reference/index.md) for detailed documentation
