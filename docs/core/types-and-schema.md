# Types and Schema

Type system for GraphQL schema definition using Python decorators and dataclasses.

**📍 Navigation**: [← Beginner Path](../tutorials/beginner-path.md) • [Queries & Mutations →](queries-and-mutations.md) • [Database API →](database-api.md)

## @fraiseql.type

**Purpose**: Define GraphQL object types from Python classes

**Signature**:
```python
import fraiseql

@fraiseql.type(
    sql_source: str | None = None,
    jsonb_column: str | None = "data",
    implements: list[type] | None = None,
    resolve_nested: bool = False
)
class TypeName:
    field1: str
    field2: int | None = None
```

**Parameters**:

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| sql_source | str \| None | None | Database table/view name for automatic query generation |
| jsonb_column | str \| None | "data" | JSONB column name containing type data. Use None for regular column tables |
| implements | list[type] \| None | None | List of GraphQL interface types this type implements |
| resolve_nested | bool | False | If True, resolve nested instances via separate database queries |

**Field Type Mappings**:

| Python Type | GraphQL Type | Notes |
|-------------|--------------|-------|
| str | String! | Non-nullable string |
| str \| None | String | Nullable string |
| int | Int! | 32-bit signed integer |
| float | Float! | Double precision float |
| bool | Boolean! | True/False |
| UUID | ID! | Auto-converted to string |
| datetime | DateTime! | ISO 8601 format |
| date | Date! | YYYY-MM-DD format |
| list[T] | [T!]! | Non-null list of non-null items |
| list[T] \| None | [T!] | Nullable list of non-null items |
| list[T \| None] | [T]! | Non-null list of nullable items |
| Decimal | Float! | High precision numbers |

## Type Mapping Flow

### Python Class to GraphQL Schema

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│  Python     │───▶│ Type        │───▶│ GraphQL     │───▶│  Client     │
│  Class      │    │ Decorator   │    │  Schema     │    │  Query      │
│             │    │             │    │             │    │             │
│ @type       │    │ @type(      │    │ type User { │    │ { user {    │
│ class User: │    │   sql_      │    │   id: ID!   │    │   id        │
│   id: UUID  │    │   source=   │    │   name:     │    │   name      │
│   name: str │    │   "v_user") │    │   String!   │    │ } }         │
└─────────────┘    └─────────────┘    └─────────────┘    └─────────────┘
```

**Type Mapping Process:**
1. **Python Class** with type hints and `@type` decorator
2. **Type Decorator** processes annotations and metadata
3. **GraphQL Schema** generated with proper types and nullability
4. **Client Queries** validated against generated schema

**[🔗 Type System Details](../diagrams/database-schema-conventions.md)** - Database naming conventions

**Examples**:

Basic type without database binding:
```python
import fraiseql
from uuid import UUID
from datetime import datetime

@fraiseql.type
class User:
    id: UUID
    email: str
    name: str | None
    created_at: datetime
    is_active: bool = True
    tags: list[str] = []
```

**Generated GraphQL Schema**:
```graphql
type User {
  id: ID!
  email: String!
  name: String
  createdAt: DateTime!
  isActive: Boolean!
  tags: [String!]!
}
```

Type with SQL source for automatic queries:
```python
import fraiseql
from uuid import UUID

@fraiseql.type(sql_source="v_user")
class User:
    id: UUID
    email: str
    name: str
```

Type with regular table columns (no JSONB):
```python
import fraiseql
from uuid import UUID

@fraiseql.type(sql_source="users", jsonb_column=None)
class User:
    id: UUID
    email: str
    name: str
    created_at: datetime
```

Type with custom JSONB column:
```python
import fraiseql
from uuid import UUID

@fraiseql.type(sql_source="tv_machine", jsonb_column="machine_data")
class Machine:
    id: UUID
    identifier: str
    serial_number: str
```

**With Custom Fields** (using @field decorator):
```python
import fraiseql
from uuid import UUID
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .types import Post

@fraiseql.type
class User:
    id: UUID
    first_name: str
    last_name: str

    @fraiseql.field(description="Full display name")
    def display_name(self) -> str:
        return f"{self.first_name} {self.last_name}"

    @fraiseql.field(description="User's posts")
    async def posts(self, info) -> list[Post]:
        db = info.context["db"]
        return await db.find("v_post", where={"user_id": self.id})
```

With nested object resolution:
```python
import fraiseql

# Department will be resolved via separate query
@fraiseql.type(sql_source="departments", resolve_nested=True)
class Department:
    id: UUID
    name: str

# Employee with department as a relation
@fraiseql.type(sql_source="employees")
class Employee:
    id: UUID
    name: str
    department_id: UUID  # Foreign key
    department: Department | None  # Will query departments table
```

With embedded nested objects (default):
```python
import fraiseql

# Department data is embedded in parent's JSONB
@fraiseql.type(sql_source="departments")
class Department:
    id: UUID
    name: str

# Employee view includes embedded department in JSONB
@fraiseql.type(sql_source="v_employees_with_dept")
class Employee:
    id: UUID
    name: str
    department: Department | None  # Uses embedded JSONB data
```

## @input

**Purpose**: Define GraphQL input types for mutations and queries

**Signature**:
```python
import fraiseql

@fraiseql.input
class InputName:
    field1: str
    field2: int | None = None
```

**Examples**:

Basic input type:
```python
import fraiseql
from uuid import UUID
from datetime import datetime

@fraiseql.type
class User:
    id: UUID
    name: str
    role: UserRole

@fraiseql.type
class Order:
    id: UUID
    status: OrderStatus
    created_at: datetime
```

Enum with integer values:
```python
@fraiseql.enum
class Priority(Enum):
    LOW = 1
    MEDIUM = 2
    HIGH = 3
    CRITICAL = 4
```

## @interface

**Purpose**: Define GraphQL interface types for polymorphism

**Signature**:
```python
import fraiseql

@fraiseql.interface
class InterfaceName:
    field1: str
    field2: int
```

**Examples**:

Basic Node interface:
```python
import fraiseql

@fraiseql.interface
class Node:
    id: UUID

@fraiseql.type(implements=[Node])
class User:
    id: UUID
    email: str
    name: str

@fraiseql.type(implements=[Node])
class Post:
    id: UUID
    title: str
    content: str
```

Interface with computed fields:
```python
import fraiseql

@fraiseql.interface
class Timestamped:
    created_at: datetime
    updated_at: datetime

    @fraiseql.field(description="Time since creation")
    def age(self) -> timedelta:
        return datetime.utcnow() - self.created_at

@fraiseql.type(implements=[Timestamped])
class Article:
    id: UUID
    title: str
    created_at: datetime
    updated_at: datetime

    @fraiseql.field(description="Time since creation")
    def age(self) -> timedelta:
        return datetime.utcnow() - self.created_at
```

Multiple interface implementation:
```python
import fraiseql

@fraiseql.interface
class Searchable:
    search_text: str

@fraiseql.interface
class Taggable:
    tags: list[str]

@fraiseql.type(implements=[Node, Searchable, Taggable])
class Document:
    id: UUID
    title: str
    content: str
    tags: list[str]

    @fraiseql.field
    def search_text(self) -> str:
        return f"{self.title} {self.content}"
```

## Scalar Types

**Built-in Scalars**:

| Import | GraphQL Type | Python Type | Format | Example |
|--------|--------------|-------------|--------|---------|
| UUID | ID | UUID | UUID string | "123e4567-..." |
| Date | Date | date | YYYY-MM-DD | "2025-10-09" |
| DateTime | DateTime | datetime | ISO 8601 | "2025-10-09T10:30:00Z" |
| EmailAddress | EmailAddress | str | RFC 5322 | "user@example.com" |
| JSON | JSON | dict/list/Any | JSON value | {"key": "value"} |

**Network Scalars**:

| Import | GraphQL Type | Description | Example |
|--------|--------------|-------------|---------|
| IpAddress | IpAddress | IPv4 or IPv6 address | "192.168.1.1" |
| CIDR | CIDR | CIDR notation network | "192.168.1.0/24" |
| MacAddress | MacAddress | MAC address | "00:1A:2B:3C:4D:5E" |
| Port | Port | Network port number | 8080 |
| Hostname | Hostname | DNS hostname | "api.example.com" |

**Other Scalars**:

| Import | GraphQL Type | Description | Example |
|--------|--------------|-------------|---------|
| LTree | LTree | PostgreSQL ltree path | "top.science.astronomy" |
| DateRange | DateRange | Date range | "[2025-01-01,2025-12-31]" |

**Usage Example**:
```python
import fraiseql

from fraiseql.types import (
    IpAddress,
    CIDR,
    MacAddress,
    Port,
    Hostname,
    LTree
)

@fraiseql.type
class NetworkConfig:
    ip_address: IpAddress
    cidr_block: CIDR
    gateway: IpAddress
    mac_address: MacAddress
    port: Port
    hostname: Hostname

@fraiseql.type
class Category:
    path: LTree  # PostgreSQL ltree for hierarchical data
    name: str
```

## Generic Types

### Connection / Edge / PageInfo (Relay Pagination)

**Purpose**: Cursor-based pagination following Relay specification

**Types**:
```python
import fraiseql

@fraiseql.type
class PageInfo:
    has_next_page: bool
    has_previous_page: bool
    start_cursor: str | None = None
    end_cursor: str | None = None
    total_count: int | None = None

@fraiseql.type
class Edge[T]:
    node: T
    cursor: str

@fraiseql.type
class Connection[T]:
    edges: list[Edge[T]]
    page_info: PageInfo
    total_count: int | None = None
```

**Usage with @connection decorator**:
```python
import fraiseql
from fraiseql.types import Connection

@fraiseql.type(sql_source="v_user")
class User:
    id: UUID
    name: str
    email: str

@fraiseql.connection(node_type=User)
@fraiseql.query
async def users_connection(
    info,
    first: int | None = None,
    after: str | None = None
) -> Connection[User]:
    pass  # Implementation handled by decorator
```

**Manual usage**:
```python
import fraiseql

from fraiseql.types import create_connection

@fraiseql.query
async def users_connection(info, first: int = 20) -> Connection[User]:
    db = info.context["db"]
    result = await db.paginate("v_user", first=first)
    return create_connection(result, User)
```

### PaginatedResponse (Offset Pagination)

**Alias**: `PaginatedResponse = Connection`

**Usage**:
```python
import fraiseql

@fraiseql.query
async def users_paginated(
    info,
    page: int = 1,
    limit: int = 20
) -> Connection[User]:
    db = info.context["db"]
    offset = (page - 1) * limit
    users = await db.find("v_user", limit=limit, offset=offset)
    total = await db.count("v_user")

    # Manual construction
    from fraiseql.types import PageInfo, Edge, Connection

    edges = [Edge(node=user, cursor=str(i)) for i, user in enumerate(users)]
    page_info = PageInfo(
        has_next_page=offset + limit < total,
        has_previous_page=page > 1,
        total_count=total
    )

    return Connection(edges=edges, page_info=page_info, total_count=total)
```

## UNSET Sentinel

**Purpose**: Distinguish between "field not provided" and "field explicitly set to None"

**Import**:
```python
from fraiseql.types import UNSET
```

**Usage in Input Types**:
```python
import fraiseql
from fraiseql.types import UNSET

@fraiseql.input
class UpdateUserInput:
    id: UUID
    name: str | None = UNSET  # Not provided by default
    email: str | None = UNSET
    bio: str | None = UNSET
```

**Usage in Mutations**:
```python
import fraiseql

@fraiseql.mutation
async def update_user(info, input: UpdateUserInput) -> User:
    db = info.context["db"]
    updates = {}

    # Only include fields that were explicitly provided
    if input.name is not UNSET:
        updates["name"] = input.name  # Could be None (clear) or str (update)
    if input.email is not UNSET:
        updates["email"] = input.email
    if input.bio is not UNSET:
        updates["bio"] = input.bio

    return await db.update_one("v_user", {"id": input.id}, updates)
```

**GraphQL Example**:
```graphql
# Mutation that only updates name (sets it to null)
mutation {
  updateUser(input: {
    id: "123"
    name: null    # Explicitly set to null - will update
    # email not provided - will not update
  }) {
    id
    name
    email
  }
}
```

## Best Practices

**Type Design**:
- Use descriptive names (User, CreateUserInput, UserConnection)
- Separate input types from output types
- Use UNSET for optional update fields
- Define enums for fixed value sets
- Use interfaces for shared behavior

**Field Naming**:
- Use snake_case in Python (auto-converts to camelCase in GraphQL)
- Prefix inputs with operation name (CreateUserInput, UpdateUserInput)
- Suffix connections with Connection (UserConnection)

**Nullability**:
- Make fields non-nullable by default (better type safety)
- Use `| None` only when field can truly be absent
- Use UNSET for "not provided" vs None for "clear this field"

**SQL Source Configuration**:
- Set sql_source for queryable types
- Set jsonb_column=None for regular table columns
- Use jsonb_column="data" (default) for CQRS/JSONB tables
- Use custom jsonb_column for non-standard column names

**Performance**:
- Use resolve_nested=True only for types that need separate database queries
- Default (resolve_nested=False) assumes data is embedded in parent JSONB
- Embedded data is faster (single query) vs nested resolution (multiple queries)

## See Also

- [Queries and Mutations](./queries-and-mutations.md) - Using types in resolvers
- [Decorators Reference](../reference/decorators.md) - Complete decorator API
- [Configuration](./configuration.md) - Type system configuration options
