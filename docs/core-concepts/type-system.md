---
← [Core Concepts](./index.md) | [Core Concepts Index](./index.md) | [Query Translation →](./query-translation.md)
---

# Type System

> **In this section:** Learn how to define type-safe GraphQL schemas using Python type hints
> **Prerequisites:** Basic understanding of Python type hints and GraphQL concepts
> **Time to complete:** 15 minutes

FraiseQL's type system provides seamless mapping between Python types and GraphQL schema, with full type safety from database to API.

## Type Decorators

FraiseQL uses decorators to define GraphQL types directly in Python:

### Output Types
```python
import fraiseql
from datetime import datetime
from uuid import UUID

@fraiseql.type
class User:
    """User type for the API."""
    id: UUID
    email: str
    name: str
    bio: str | None  # Modern Python union syntax
    created_at: datetime
    is_active: bool = True  # Default values supported
    roles: list[str] = field(default_factory=list)
```

## Automatic Documentation

FraiseQL automatically extracts Python docstrings and converts them to GraphQL schema descriptions that appear in Apollo Studio and GraphQL introspection.

### Type Descriptions
```python
@fraiseql.type
class User:
    """A user account with authentication and profile information."""
    id: UUID
    email: str
    name: str
```

The docstring automatically becomes the GraphQL type description visible in Apollo Studio.

### Query & Mutation Descriptions
```python
@fraiseql.query
async def get_user_profile(info, user_id: UUID) -> User:
    """Retrieve a user's complete profile with preferences and settings."""
    return await db.get_user(user_id)

@fraiseql.mutation
class CreateUser:
    """Create a new user account with validation and welcome email."""

    input: CreateUserInput
    success: CreateUserSuccess
    failure: CreateUserError

    async def resolve(self, info):
        # Implementation
        pass
```

All docstrings are automatically cleaned and formatted for GraphQL schema documentation.

### Input Types
```python
@fraiseql.input
class CreateUserInput:
    """Input for creating a new user."""
    email: str
    name: str
    password: str
    bio: str | None = None  # Optional fields
```

### Success/Failure Types
```python
@fraiseql.success
class CreateUserSuccess:
    """Success response for user creation."""
    user: User
    message: str = "User created successfully"

@fraiseql.failure
class CreateUserError:
    """Error response for user creation."""
    message: str
    code: str = "USER_CREATION_FAILED"
```

### Interfaces
```python
@fraiseql.interface
class Node:
    """Interface for objects with an ID."""
    id: UUID

@fraiseql.type
class User(Node):  # Implements Node interface
    email: str
    name: str
```

### Enums
```python
from enum import Enum

@fraiseql.enum
class UserRole(Enum):
    ADMIN = "admin"
    EDITOR = "editor"
    VIEWER = "viewer"

@fraiseql.type
class User:
    id: UUID
    role: UserRole
```

## Python to GraphQL Type Mapping

FraiseQL automatically maps Python types to GraphQL:

| Python Type | GraphQL Type | Notes |
|------------|--------------|-------|
| `str` | `String!` | Non-nullable string |
| `str \| None` | `String` | Nullable string |
| `int` | `Int!` | 32-bit integer |
| `float` | `Float!` | Double precision |
| `bool` | `Boolean!` | Boolean value |
| `UUID` | `ID!` | UUID mapped to ID |
| `datetime` | `DateTime!` | ISO 8601 string |
| `date` | `Date!` | ISO date string |
| `time` | `Time!` | ISO time string |
| `list[T]` | `[T!]!` | Non-null list of non-null items |
| `list[T \| None]` | `[T]!` | Non-null list of nullable items |
| `dict[str, Any]` | `JSONObject!` | JSON object scalar |
| `Any` | `JSON` | Arbitrary JSON |

## Built-in Scalar Types

### ID Type for UUIDs
FraiseQL maps Python `UUID` to GraphQL `ID`:

```python
from uuid import UUID

@fraiseql.type
class User:
    id: UUID  # Becomes ID! in GraphQL
    parent_id: UUID | None  # Becomes ID in GraphQL
```

### DateTime Types
```python
from datetime import datetime, date, time

@fraiseql.type
class Event:
    id: UUID
    starts_at: datetime  # ISO 8601: "2024-01-15T14:30:00Z"
    event_date: date     # ISO date: "2024-01-15"
    event_time: time     # ISO time: "14:30:00"
```

### JSON Types
```python
@fraiseql.type
class Configuration:
    id: UUID
    settings: dict[str, Any]  # Arbitrary JSON object
    metadata: Any  # Any JSON value
```

## Custom Scalar Types

Define custom scalars for specialized types:

```python
from fraiseql.scalars import Scalar
from decimal import Decimal

# Define a custom Decimal scalar
class DecimalScalar(Scalar):
    """Decimal number with arbitrary precision."""

    @staticmethod
    def serialize(value: Decimal) -> str:
        return str(value)

    @staticmethod
    def parse_value(value: str) -> Decimal:
        return Decimal(value)

    @staticmethod
    def parse_literal(ast) -> Decimal:
        return Decimal(ast.value)

# Use in types
@fraiseql.type
class Product:
    id: UUID
    price: Decimal  # Uses custom scalar
    quantity: int
```

### Common Custom Scalars

```python
# Email validation
from fraiseql.scalars import EmailScalar

@fraiseql.type
class User:
    id: UUID
    email: EmailScalar  # Validates email format

# URL validation
from fraiseql.scalars import URLScalar

@fraiseql.type
class Profile:
    website: URLScalar | None

# PostgreSQL-specific types
from fraiseql.scalars import JSONB, INET, CIDR, MacAddress, LTree

@fraiseql.type
class ServerLog:
    data: JSONB  # PostgreSQL JSONB
    client_ip: INET  # IP address
    network: CIDR  # Network range
    mac_address: MacAddress  # MAC address
    path: LTree  # Hierarchical path
```

## Complex Types with Modern Syntax

### Union Types
```python
# Using Python 3.10+ union syntax
@fraiseql.type
class SearchResult:
    id: UUID
    title: str
    content: str | None  # Optional field
    tags: list[str] | None  # Optional list

# Result unions for mutations
CreateUserResult = CreateUserSuccess | CreateUserError

@fraiseql.mutation
async def create_user(
    info,
    input: CreateUserInput
) -> CreateUserResult:
    # Returns either success or error type
    ...
```

### Nested Types
```python
@fraiseql.type
class Address:
    street: str
    city: str
    country: str
    postal_code: str

@fraiseql.type
class User:
    id: UUID
    name: str
    address: Address | None  # Nested type
    addresses: list[Address]  # List of nested types
```

### Generic Types
```python
from typing import Generic, TypeVar

T = TypeVar('T')

@fraiseql.type
class PaginatedResult(Generic[T]):
    items: list[T]
    total: int
    page: int
    per_page: int

# Usage
@fraiseql.type
class UserList(PaginatedResult[User]):
    pass
```

## Field Configuration

### Using fraise_field
```python
from fraiseql import fraise_field

@fraiseql.type
class User:
    id: UUID

    # Add descriptions
    email: str = fraise_field(description="User's email address")

    # Deprecation
    old_field: str = fraise_field(
        deprecation_reason="Use newField instead"
    )

    # Default values
    is_active: bool = fraise_field(default=True)

    # Default factories for mutable types
    roles: list[str] = fraise_field(default_factory=list)

    # Permissions
    sensitive_data: str = fraise_field(
        permissions=["admin:read"]
    )
```

### Computed Fields
```python
@fraiseql.type
class Post:
    id: UUID
    title: str
    content: str

    @property
    def word_count(self) -> int:
        """Computed field for word count."""
        return len(self.content.split())

    @property
    def reading_time(self) -> int:
        """Estimated reading time in minutes."""
        return max(1, self.word_count // 200)
```

## Input Types and Validation

### Basic Validation
```python
from fraiseql import validate

@fraiseql.input
class CreatePostInput:
    title: str = fraise_field(
        min_length=1,
        max_length=200,
        description="Post title"
    )
    content: str = fraise_field(
        min_length=10,
        description="Post content in Markdown"
    )
    tags: list[str] = fraise_field(
        max_items=10,
        default_factory=list
    )

    @validate
    def validate_tags(self):
        """Custom validation logic."""
        if self.tags:
            for tag in self.tags:
                if len(tag) > 50:
                    raise ValueError(f"Tag '{tag}' is too long")
```

### Partial Update Inputs
```python
@fraiseql.input
class UpdateUserInput:
    """All fields optional for partial updates."""
    name: str | None = None
    bio: str | None = None
    email: str | None = None
    is_active: bool | None = None
```

## Type Modifiers and Optionality

### Non-Nullable vs Nullable
```python
@fraiseql.type
class User:
    # Required fields (non-nullable)
    id: UUID  # ID!
    email: str  # String!

    # Optional fields (nullable)
    bio: str | None  # String
    avatar_url: str | None  # String

    # Required list of required items
    roles: list[str]  # [String!]!

    # Required list of optional items
    tags: list[str | None]  # [String]!

    # Optional list of required items
    posts: list[Post] | None  # [Post!]
```

### Default Values
```python
@fraiseql.type
class Settings:
    # Simple defaults
    theme: str = "light"
    notifications: bool = True

    # Default factory for mutable types
    blocked_users: list[str] = field(default_factory=list)
    preferences: dict[str, Any] = field(default_factory=dict)

    # Computed default
    created_at: datetime = field(default_factory=datetime.now)
```

## Type Registration and Discovery

### Automatic Registration
Types are automatically registered when decorated:

```python
# models.py
@fraiseql.type
class User:
    id: UUID
    name: str

# The User type is now registered globally
```

### Manual Registration
```python
from fraiseql.gql.builders.registry import GlobalRegistry

# Register a type manually
GlobalRegistry.register_type("CustomUser", User)

# Get registered type
user_type = GlobalRegistry.get_type("User")
```

### Type Inspection
```python
from fraiseql import get_schema

# Get the complete GraphQL schema
schema = get_schema()

# Introspect types
for type_name in schema.type_map:
    graphql_type = schema.type_map[type_name]
    print(f"{type_name}: {graphql_type}")
```

## Integration with Database Views

Types should mirror your database view structure:

### Database View
```sql
CREATE OR REPLACE VIEW v_user AS
SELECT
    id,
    jsonb_build_object(
        '__typename', 'User',
        'id', id,
        'email', email,
        'name', name,
        'created_at', created_at,
        'is_active', is_active
    ) AS data
FROM tb_users;
```

### Corresponding Python Type
```python
@fraiseql.type
class User:
    """Matches v_user view structure."""
    id: UUID
    email: str
    name: str
    created_at: datetime
    is_active: bool
```

## Type Inheritance

```python
@fraiseql.interface
class Timestamped:
    """Interface for timestamped objects."""
    created_at: datetime
    updated_at: datetime

@fraiseql.interface
class Authored:
    """Interface for authored content."""
    author_id: UUID

@fraiseql.type
class Post(Timestamped, Authored):
    """Post implements multiple interfaces."""
    id: UUID
    title: str
    content: str
    # Inherits created_at, updated_at from Timestamped
    # Inherits author_id from Authored
```

## Best Practices

### 1. Match Database Structure
Ensure types align with your database views:
```python
# If view has these fields, type should too
@fraiseql.type
class User:
    id: UUID  # matches 'id' in view
    email: str  # matches 'email' in view
    created_at: datetime  # matches 'created_at' in view
```

### 2. Use Modern Python Syntax
```python
# Good: Modern union syntax
bio: str | None

# Avoid: Old-style Optional
from typing import Optional  # Don't do this in modern Python
bio: Optional[str]  # Less preferred - use bio: str | None instead
```

### 3. Document Types
```python
@fraiseql.type
class User:
    """User account in the system."""

    id: UUID
    email: str = fraise_field(
        description="Primary email address"
    )
    verified_at: datetime | None = fraise_field(
        description="When email was verified"
    )
```

### 4. Group Related Types
```python
# user_types.py
@fraiseql.type
class User: ...

@fraiseql.input
class CreateUserInput: ...

@fraiseql.success
class CreateUserSuccess: ...

@fraiseql.failure
class CreateUserError: ...
```

### 5. Use Enums for Fixed Values
```python
@fraiseql.enum
class PostStatus(Enum):
    DRAFT = "draft"
    PUBLISHED = "published"
    ARCHIVED = "archived"

@fraiseql.type
class Post:
    status: PostStatus  # Type-safe status
```

## GraphQL Filtering and Where Inputs

FraiseQL automatically generates GraphQL `WhereInput` types for filtering based on your field types. The available filtering operators depend on the field's data type.

### Automatic Where Input Generation

For any type decorated with `@fraiseql.type`, FraiseQL automatically creates filtering inputs:

```python
@fraiseql.type
class User:
    id: UUID
    name: str
    email: str
    created_at: datetime
    is_active: bool

# Automatically generates UserWhereInput with appropriate filters for each field
```

### Standard Filter Operations by Type

| Python Type | Available Operators | Example |
|------------|-------------------|---------|
| `str` | `eq`, `neq`, `contains`, `startswith`, `endswith`, `in_`, `nin`, `isnull` | `name: {contains: "john"}` |
| `int`, `float` | `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `in_`, `nin`, `isnull` | `age: {gte: 18}` |
| `bool` | `eq`, `neq`, `isnull` | `is_active: {eq: true}` |
| `UUID` | `eq`, `neq`, `in_`, `nin`, `isnull` | `id: {eq: "123..."}` |
| `datetime`, `date` | `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `in_`, `nin`, `isnull` | `created_at: {gte: "2024-01-01"}` |

### Restricted Filter Types for Exotic Scalars

**⚠️ Breaking Change in v0.3.7**: Exotic scalar types now use restricted filter sets that only expose operators that work correctly with PostgreSQL type normalization.

#### Network Address Types
```python
from fraiseql.types import IpAddress, CIDR

@fraiseql.type
class Server:
    ip_address: IpAddress  # Uses NetworkAddressFilter
    network: CIDR         # Uses NetworkAddressFilter
```

**NetworkAddressFilter** only exposes: `eq`, `neq`, `in_`, `nin`, `isnull`
- ❌ **Removed**: `contains`, `startswith`, `endswith` (broken due to CIDR notation like `/32`)
- ✅ **Working**: Exact matching and list operations

```graphql
# ✅ These work correctly
servers(where: {
  ip_address: {eq: "10.0.0.1"}
  network: {in: ["192.168.1.0/24", "10.0.0.0/8"]}
})

# ❌ These were removed (never worked correctly)
# servers(where: {ip_address: {contains: "10.0"}})  # No longer available
```

#### MAC Address Types
```python
from fraiseql.types import MacAddress

@fraiseql.type
class NetworkDevice:
    mac_address: MacAddress  # Uses MacAddressFilter
```

**MacAddressFilter** only exposes: `eq`, `neq`, `in_`, `nin`, `isnull`
- ❌ **Removed**: `contains`, `startswith`, `endswith` (broken due to MAC normalization)
- ✅ **Working**: Exact matching and list operations

#### Hierarchical Path Types
```python
from fraiseql.types import LTree

@fraiseql.type
class Category:
    path: LTree  # Uses LTreeFilter
```

**LTreeFilter** only exposes: `eq`, `neq`, `isnull` (most conservative)
- ❌ **Removed**: All pattern matching and list operations
- 🔄 **Future**: Will add proper ltree operators (`ancestor_of`, `descendant_of`, `matches_lquery`)

#### Date Range Types
```python
from fraiseql.types import DateRange

@fraiseql.type
class Event:
    date_range: DateRange  # Uses DateRangeFilter
```

**DateRangeFilter** only exposes: `eq`, `neq`, `isnull`
- 🔄 **Future**: Will add proper range operators (`contains_date`, `overlaps`, `adjacent`)

### Migration Guide for v0.3.7

If you were using pattern matching operators with exotic types:

```python
# Before v0.3.7 (this was broken but allowed)
servers = await repo.find("v_server", where={
    "ip_address__contains": "192.168"  # Never worked correctly
})

# v0.3.7+ (use exact matching instead)
servers = await repo.find("v_server", where={
    "ip_address": "192.168.1.100"  # Works correctly
})

# Or use IN operator for multiple IPs
servers = await repo.find("v_server", where={
    "ip_address__in": ["192.168.1.100", "192.168.1.101"]
})
```

### Custom Filtering Logic

For complex filtering needs with exotic types, implement custom resolvers:

```python
@fraiseql.query
async def servers_in_network(
    info,
    network_prefix: str
) -> list[Server]:
    """Custom filtering for network ranges."""
    repo = info.context["repo"]

    # Use PostgreSQL network operators directly
    return await repo.raw_query("""
        SELECT jsonb_build_object('id', id, 'ip_address', ip_address)
        FROM v_server
        WHERE ip_address <<= %s::inet
    """, [f"{network_prefix}/24"])
```

## Testing Types

```python
import pytest
from fraiseql import get_schema

def test_user_type_registration():
    """Test that User type is properly registered."""
    schema = get_schema()
    user_type = schema.type_map.get("User")

    assert user_type is not None
    assert "id" in user_type.fields
    assert "email" in user_type.fields

def test_type_field_nullability():
    """Test field nullability."""
    schema = get_schema()
    user_type = schema.type_map["User"]

    # Required field
    assert user_type.fields["id"].type.of_type is None

    # Optional field
    bio_field = user_type.fields["bio"]
    assert bio_field.type.of_type is not None

def test_restricted_filter_types():
    """Test that exotic types use restricted filters."""
    schema = get_schema()

    # Check that NetworkDevice generates proper where input
    where_input_type = schema.type_map.get("NetworkDeviceWhereInput")
    assert where_input_type is not None

    # MAC address field should use MacAddressFilter
    mac_field = where_input_type.fields["mac_address"]
    mac_filter_type = mac_field.type.of_type

    # Should have eq, neq, but not contains
    assert "eq" in mac_filter_type.fields
    assert "neq" in mac_filter_type.fields
    assert "contains" not in mac_filter_type.fields  # Removed in v0.3.7
```

## Next Steps

- Learn about [Query Translation](./query-translation.md) to see how types are used in queries
- Explore [Database Views](./database-views.md) to understand the data source
- See complete examples in the [Blog API Tutorial](../tutorials/blog-api.md)
