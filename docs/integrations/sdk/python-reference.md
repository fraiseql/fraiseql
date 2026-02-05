# FraiseQL Python SDK Reference

**Status**: Production-Ready | **Python Version**: 3.10+ | **SDK Version**: 2.0.0+
**Last Updated**: 2026-02-05 | **Maintained By**: FraiseQL Community

Complete API reference for the FraiseQL Python SDK. This guide covers the complete Python authoring interface for building type-safe GraphQL APIs with Python decorators and type hints.

## Quick Start

```bash
# Installation
pip install fraiseql

# Or with uv (recommended)
uv add fraiseql
```

**Requirements**:
- Python 3.10 or later
- Type annotations (PEP 484, 586, 604)
- pip or uv package manager

**First Schema** (30 seconds):

```python
import fraiseql

@fraiseql.type
class User:
    id: int
    name: str

@fraiseql.query(sql_source="v_users")
def users(limit: int = 10) -> list[User]:
    """Get all users."""
    pass

fraiseql.export_schema("schema.json")
```

Export and deploy to your FraiseQL server:

```bash
fraiseql-cli compile schema.json fraiseql.toml
fraiseql-server --schema schema.compiled.json
```

---

## Quick Reference Table

| Feature | Decorator | Purpose | Returns |
|---------|-----------|---------|---------|
| **Types** | `@fraiseql.type` | GraphQL object types | JSON schema |
| **Queries** | `@fraiseql.query()` | Read operations (SELECT) | Single or list |
| **Mutations** | `@fraiseql.mutation()` | Write operations (INSERT/UPDATE/DELETE) | Type result |
| **Fact Tables** | `@fraiseql.fact_table()` | Analytics tables (OLAP) | Aggregation schema |
| **Aggregate Queries** | `@fraiseql.aggregate_query()` | Analytics queries | Aggregated results |
| **Observers** | `@fraiseql.observer()` | Event webhooks (async) | Event response |
| **Security** | `@fraiseql.security()` | RBAC and access control | Auth metadata |
| **Subscriptions** | `@fraiseql.subscription()` | Real-time pub/sub | Event stream |
| **Validators** | `@fraiseql.validator()` | Field validation | Validation result |

---

## Type System

### 1. The `@fraiseql.type` Decorator

Define GraphQL object types using Python classes with type annotations.

**Signature:**

```python
@fraiseql.type
class MyType:
    field1: int
    field2: str
    field3: bool
```

**Key Features**:

- **Type Annotations Required**: All fields must have Python type hints
- **Nullability**: Use `T | None` to indicate optional fields (Python 3.10+)
- **Nested Types**: Reference other `@fraiseql.type` classes
- **Lists**: Use `list[T]` for array types
- **Docstrings**: Become GraphQL type descriptions
- **No Inheritance**: Each type is independent (flat schema)

**Examples**:

```python
# ✅ Simple type
@fraiseql.type
class User:
    """A user account."""
    id: int
    username: str
    email: str

# ✅ With nullable fields
@fraiseql.type
class Post:
    """A blog post."""
    id: int
    title: str
    body: str
    published_at: str | None  # Optional field

# ✅ With lists
@fraiseql.type
class Blog:
    """A blog with multiple posts."""
    id: int
    name: str
    posts: list[Post]
    tags: list[str]

# ✅ Nested types
@fraiseql.type
class Address:
    """A physical address."""
    street: str
    city: str
    state: str
    postal_code: str

@fraiseql.type
class Company:
    """A company with address."""
    id: int
    name: str
    headquarters: Address
    employees: list[User]

# ✅ With docstrings for GraphQL descriptions
@fraiseql.type
class Product:
    """A product in the catalog.

    Fields:
    - id: Unique product identifier
    - name: Product name (max 255 chars)
    - price: Product price in USD
    - in_stock: Availability status
    """
    id: int
    name: str
    price: float
    in_stock: bool
```

**Advanced Type Features**:

```python
# Nullable list elements
@fraiseql.type
class UserSearchResult:
    """Results with potential nulls."""
    matches: list[User | None]  # List can contain nulls

# Complex nested structure
@fraiseql.type
class Department:
    """Represents a department."""
    id: int
    name: str
    manager: User | None
    members: list[User]
    budget: float
    created_at: str

# Multiple levels of nesting
@fraiseql.type
class Organization:
    """An organization with multiple departments."""
    id: int
    name: str
    departments: list[Department]
```

### 2. Type Mapping: Python ↔ GraphQL

FraiseQL automatically maps Python types to GraphQL types:

| Python Type | GraphQL Type | Notes |
|-------------|-------------|-------|
| `int` | `Int` | 32-bit signed integer |
| `float` | `Float` | IEEE 754 floating point |
| `str` | `String` | UTF-8 text |
| `bool` | `Boolean` | True/False |
| `list[int]` | `[Int!]!` | Non-empty list of non-null ints |
| `list[int \| None]` | `[Int]!` | Non-empty list of nullable ints |
| `int \| None` | `Int` | Nullable int |
| `@fraiseql.type class User` | `User!` | Custom object type (non-null) |
| `User \| None` | `User` | Nullable custom type |
| `list[User]` | `[User!]!` | Non-empty list of users |
| `list[User \| None]` | `[User]!` | List with nullable users |

**Scalar Type Extensions** (60+ available):

```python
from fraiseql.scalars import (
    DateTime,  # ISO 8601 datetime
    Date,      # ISO 8601 date
    Time,      # ISO 8601 time
    UUID,      # UUID v4
    JSON,      # Arbitrary JSON
    BigInt,    # 64-bit integer
    Decimal,   # Arbitrary precision
)

@fraiseql.type
class Event:
    id: UUID
    occurred_at: DateTime
    metadata: JSON | None
    amount: Decimal
```

### 3. Modern Python 3.10+ Type Hints

FraiseQL requires modern type hint syntax:

```python
# ✅ Correct (Python 3.10+ union syntax)
def get_user(user_id: int) -> User | None:
    pass

def get_items() -> list[int]:
    pass

# ❌ Incorrect (deprecated style)
def get_user(user_id: int) -> Optional[User]:  # Don't use Optional
    pass

def get_items() -> List[int]:  # Use list, not List
    pass
```

---

## Operations

### 1. Queries: Read Operations

Queries are read-only operations that fetch data from views.

**Signature:**

```python
@fraiseql.query(sql_source="view_name")
def query_name(arg1: int, arg2: str = "default") -> ResultType:
    """Query description."""
    pass
```

**Parameters**:

- `sql_source` (optional): SQL view or function name to execute
- `auto_params` (optional): Dictionary of parameter configurations
- `cache_ttl` (optional): Cache results for N seconds (0 = no cache)

**Examples**:

```python
# Simple list query
@fraiseql.query(sql_source="v_users")
def users(limit: int = 10) -> list[User]:
    """Get all users with pagination."""
    pass

# Single result query
@fraiseql.query(sql_source="v_user_by_id")
def user(id: int) -> User | None:
    """Get a user by ID, returns null if not found."""
    pass

# Query with multiple parameters
@fraiseql.query(sql_source="v_search_users")
def search_users(
    name: str,
    email: str | None = None,
    is_active: bool = True,
    limit: int = 20,
    offset: int = 0
) -> list[User]:
    """Search users by name and optionally email."""
    pass

# Query with explicit parameter configuration
@fraiseql.query(
    sql_source="v_analytics",
    auto_params={
        "start_date": {"type": "Date", "required": True},
        "end_date": {"type": "Date", "required": False},
        "limit": {"type": "Int", "default": 100}
    }
)
def analytics(
    start_date: str,
    end_date: str | None = None,
    limit: int = 100
) -> list[dict]:
    """Get analytics for a date range."""
    pass

# Query without SQL source (for computed fields)
@fraiseql.query
def server_time() -> str:
    """Get current server time."""
    pass

# Cached query (results cached for 300 seconds)
@fraiseql.query(sql_source="v_trending", cache_ttl=300)
def trending_items(limit: int = 10) -> list[Item]:
    """Get trending items (cached for 5 minutes)."""
    pass
```

**GraphQL Generated**:

```graphql
type Query {
  users(limit: Int = 10): [User!]!
  user(id: Int!): User
  searchUsers(
    name: String!
    email: String
    isActive: Boolean = true
    limit: Int = 20
    offset: Int = 0
  ): [User!]!
  serverTime: String!
}
```

**Query Argument Handling**:

Arguments follow Python function signature conventions:

```python
@fraiseql.query(sql_source="v_data")
def get_data(
    required_arg: int,           # Required (no default)
    optional_arg: str = "default",  # Optional (has default)
    nullable_arg: int | None = None # Nullable + optional
) -> list[dict]:
    """Demonstrates all argument types."""
    pass
```

Generates:

```graphql
type Query {
  getData(
    requiredArg: Int!
    optionalArg: String = "default"
    nullableArg: Int
  ): [dict!]!
}
```

### 2. Mutations: Write Operations

Mutations are write operations that modify data (CREATE, UPDATE, DELETE).

**Signature:**

```python
@fraiseql.mutation(
    sql_source="function_name",
    operation="CREATE"  # CREATE | UPDATE | DELETE | CUSTOM
)
def mutation_name(arg1: str, arg2: int) -> ResultType:
    """Mutation description."""
    pass
```

**Parameters**:

- `sql_source` (required): SQL function name to execute
- `operation` (optional): Type of operation
  - `"CREATE"` - Insert new data
  - `"UPDATE"` - Modify existing data
  - `"DELETE"` - Remove data
  - `"CUSTOM"` - Custom operation
- `transaction_isolation` (optional): Transaction isolation level

**Examples**:

```python
# Create mutation
@fraiseql.mutation(
    sql_source="fn_create_user",
    operation="CREATE"
)
def create_user(name: str, email: str) -> User:
    """Create a new user account."""
    pass

# Update mutation
@fraiseql.mutation(
    sql_source="fn_update_user",
    operation="UPDATE"
)
def update_user(
    id: int,
    name: str | None = None,
    email: str | None = None
) -> User:
    """Update a user's profile."""
    pass

# Delete mutation
@fraiseql.mutation(
    sql_source="fn_delete_user",
    operation="DELETE"
)
def delete_user(id: int) -> bool:
    """Delete a user and return success status."""
    pass

# Batch operation
@fraiseql.mutation(
    sql_source="fn_bulk_update_users",
    operation="UPDATE"
)
def bulk_update_users(ids: list[int], status: str) -> list[User]:
    """Update multiple users' status at once."""
    pass

# Complex mutation with nested result
@fraiseql.mutation(
    sql_source="fn_create_post_with_tags",
    operation="CREATE"
)
def create_post(
    user_id: int,
    title: str,
    body: str,
    tags: list[str]
) -> Post:
    """Create a post with multiple tags."""
    pass

# Mutation with transaction isolation
@fraiseql.mutation(
    sql_source="fn_transfer_funds",
    operation="CUSTOM",
    transaction_isolation="SERIALIZABLE"
)
def transfer_funds(
    from_account: int,
    to_account: int,
    amount: float
) -> bool:
    """Transfer funds between accounts (strict isolation)."""
    pass
```

**GraphQL Generated**:

```graphql
type Mutation {
  createUser(name: String!, email: String!): User!
  updateUser(
    id: Int!
    name: String
    email: String
  ): User!
  deleteUser(id: Int!): Boolean!
  bulkUpdateUsers(ids: [Int!]!, status: String!): [User!]!
}
```

### 3. Subscriptions: Real-time Events

Subscriptions provide real-time data via WebSocket or Server-Sent Events.

**Signature:**

```python
@fraiseql.subscription(
    topic="channel_name",
    message_type=MessageType
)
def subscription_name(filter_arg: str | None = None) -> MessageType:
    """Subscription description."""
    pass
```

**Examples**:

```python
@fraiseql.type
class UserCreatedEvent:
    """Fired when a new user is created."""
    user: User
    created_at: str

@fraiseql.subscription(
    topic="users.created",
    message_type=UserCreatedEvent
)
def on_user_created() -> UserCreatedEvent:
    """Subscribe to new user creation events."""
    pass

@fraiseql.subscription(
    topic="users.updated",
    message_type=User
)
def on_user_updated(user_id: int) -> User:
    """Subscribe to updates for a specific user."""
    pass

@fraiseql.subscription(
    topic="messages",
    message_type=Message
)
def messages(room_id: int | None = None) -> Message:
    """Subscribe to new messages, optionally filtered by room."""
    pass
```

---

## Advanced Features

### 1. Fact Tables: Analytics

Define analytics tables for OLAP queries.

**Signature:**

```python
@fraiseql.fact_table(
    table_name="tf_sales",
    measures=["revenue", "quantity"],
    dimension_paths=[
        {
            "name": "category",
            "json_path": "data->>'category'",
            "data_type": "text"
        }
    ]
)
@fraiseql.type
class Sale:
    id: int
    revenue: float
    quantity: int
```

**Parameters**:

- `table_name` (required): SQL table name (must start with `tf_`)
- `measures` (required): List of numeric column names
- `dimension_column` (optional): JSONB column name (default: "data")
- `dimension_paths` (optional): Dimension definitions
- `denormalized_columns` (optional): Fast-access filter columns

**Examples**:

```python
# Multi-dimensional fact table
@fraiseql.fact_table(
    table_name="tf_sales",
    measures=["revenue", "quantity", "cost", "margin"],
    dimension_column="attributes",
    dimension_paths=[
        {
            "name": "category",
            "json_path": "attributes->>'category'",
            "data_type": "text"
        },
        {
            "name": "subcategory",
            "json_path": "attributes->>'subcategory'",
            "data_type": "text"
        },
        {
            "name": "region",
            "json_path": "attributes->>'region'",
            "data_type": "text"
        },
        {
            "name": "salesperson",
            "json_path": "attributes->>'salesperson'",
            "data_type": "text"
        }
    ],
    denormalized_columns=["customer_id", "created_at"]
)
@fraiseql.type
class Sale:
    """A sales fact record."""
    id: int
    revenue: float          # Measure for SUM/AVG
    quantity: int          # Measure for SUM/COUNT
    cost: float           # Measure for SUM
    margin: float         # Derived measure
    customer_id: int      # Denormalized for filtering
    created_at: str       # Denormalized for time filtering

# Real-time events fact table
@fraiseql.fact_table(
    table_name="tf_events",
    measures=["count", "duration"],
    dimension_paths=[
        {
            "name": "event_type",
            "json_path": "metadata->>'type'",
            "data_type": "text"
        },
        {
            "name": "severity",
            "json_path": "metadata->>'severity'",
            "data_type": "text"
        }
    ]
)
@fraiseql.type
class Event:
    """An analytics event."""
    id: int
    count: int
    duration: float
    user_id: int
    occurred_at: str
```

**SQL Table Pattern**:

```sql
-- Fact table for sales analytics
CREATE TABLE tf_sales (
    id BIGSERIAL PRIMARY KEY,

    -- Measures (numeric, aggregatable)
    revenue DECIMAL(10,2) NOT NULL,
    quantity INT NOT NULL,
    cost DECIMAL(10,2) NOT NULL,
    margin DECIMAL(10,2) NOT NULL,

    -- Dimensions (in JSONB)
    attributes JSONB NOT NULL,

    -- Denormalized filters (indexed for performance)
    customer_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,

    -- Metadata
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes on denormalized columns for filtering
CREATE INDEX ON tf_sales(customer_id);
CREATE INDEX ON tf_sales(created_at);
CREATE INDEX ON tf_sales(EXTRACT(YEAR FROM created_at),
                        EXTRACT(MONTH FROM created_at));
```

### 2. Aggregate Queries: OLAP Analytics

Define flexible analytics queries on fact tables.

**Signature:**

```python
@fraiseql.aggregate_query(
    fact_table="tf_sales",
    auto_group_by=True,
    auto_aggregates=True
)
@fraiseql.query
def analytics_query() -> list[dict]:
    """Analytics query description."""
    pass
```

**Parameters**:

- `fact_table` (required): Fact table name (from `@fraiseql.fact_table`)
- `auto_group_by` (optional): Auto-generate GROUP BY fields (default: True)
- `auto_aggregates` (optional): Auto-generate aggregates (default: True)
- `allow_empty_group_by` (optional): Allow queries without grouping (default: False)

**Examples**:

```python
# Sales by category
@fraiseql.aggregate_query(
    fact_table="tf_sales",
    auto_group_by=True,
    auto_aggregates=True
)
@fraiseql.query
def sales_by_category(
    start_date: str | None = None,
    end_date: str | None = None,
    limit: int = 100
) -> list[dict]:
    """Sales aggregated by category and time."""
    pass

# Custom aggregation (manual configuration)
@fraiseql.aggregate_query(
    fact_table="tf_sales",
    auto_group_by=False,
    auto_aggregates=False
)
@fraiseql.query
def custom_sales_analysis() -> list[dict]:
    """Fully custom sales aggregation."""
    pass

# Revenue analysis with filtering
@fraiseql.aggregate_query(
    fact_table="tf_sales"
)
@fraiseql.query
def revenue_analysis(
    min_revenue: float = 0,
    region: str | None = None
) -> list[dict]:
    """Analyze revenue by multiple dimensions."""
    pass
```

**Generated GraphQL**:

```graphql
type SalesAggregate {
  # Dimensions
  category: String
  subcategory: String
  region: String
  salesperson: String
  created_at_year: Int
  created_at_month: Int
  created_at_day: Int
  created_at_week: Int

  # Aggregates
  revenue_sum: Float
  revenue_avg: Float
  revenue_min: Float
  revenue_max: Float
  quantity_sum: Int
  quantity_avg: Float
  cost_sum: Float
  margin_sum: Float
  count: Int
}

type Query {
  salesByCategory(
    startDate: String
    endDate: String
    limit: Int = 100
    groupBy: [String!]
    aggregates: [String!]
    where: SalesWhereInput
    having: SalesHavingInput
    orderBy: [String!]
    offset: Int = 0
  ): [SalesAggregate!]!
}
```

### 3. Observers: Event Webhooks

Observers trigger async webhooks when events occur.

**Signature:**

```python
@fraiseql.observer(
    on="mutation_name",
    trigger="success"  # success | failure | always
)
def observer_name(event: ObserverEvent) -> bool:
    """Observer description."""
    pass
```

**Examples**:

```python
@fraiseql.type
class UserCreatedEvent:
    """Event fired when a user is created."""
    user: User
    timestamp: str

# Send webhook after user creation
@fraiseql.observer(
    on="create_user",
    trigger="success"
)
def notify_on_user_created(event: UserCreatedEvent) -> bool:
    """Send notification when user is created."""
    # This gets compiled to call webhooks
    pass

# Log all user updates
@fraiseql.observer(
    on="update_user",
    trigger="always"
)
def log_user_update(event: dict) -> bool:
    """Log all user update attempts."""
    pass
```

### 4. Security & RBAC

Control access using role-based access control.

**Signature:**

```python
@fraiseql.security(
    requires_auth=True,
    roles=["admin", "user"],
    field_level={"sensitive_field": ["admin"]}
)
def operation_name() -> ResultType:
    pass
```

**Examples**:

```python
# Public query (no auth required)
@fraiseql.query(sql_source="v_public_data")
@fraiseql.security(requires_auth=False)
def public_data(limit: int = 10) -> list[PublicData]:
    """Publicly accessible data."""
    pass

# Admin-only query
@fraiseql.query(sql_source="v_admin_stats")
@fraiseql.security(requires_auth=True, roles=["admin"])
def admin_stats() -> dict:
    """Administrative statistics (admin only)."""
    pass

# User query with field-level security
@fraiseql.type
class UserProfile:
    id: int
    name: str
    email: str
    ssn: str  # Sensitive field

@fraiseql.query(sql_source="v_user_profile")
@fraiseql.security(
    requires_auth=True,
    field_level={
        "ssn": ["admin"],  # Only admins see SSN
        "email": ["admin", "user"]  # Users see email
    }
)
def user_profile(id: int) -> UserProfile | None:
    """Get user profile with field-level access control."""
    pass

# Multi-tenant query
@fraiseql.query(sql_source="v_tenant_data")
@fraiseql.security(requires_auth=True, multi_tenant=True)
def my_data(limit: int = 10) -> list[TenantData]:
    """Get only current tenant's data."""
    pass
```

---

## Scalar Types Reference

FraiseQL supports 60+ scalar types. Common examples:

```python
from fraiseql.scalars import (
    # Standard types
    Int,           # 32-bit signed integer
    Float,         # IEEE 754 floating point
    String,        # UTF-8 text
    Boolean,       # True/False

    # Date/Time types
    Date,          # ISO 8601 date (YYYY-MM-DD)
    Time,          # ISO 8601 time (HH:MM:SS)
    DateTime,      # ISO 8601 datetime with timezone
    Timestamp,     # Unix timestamp (ms)

    # Numeric types
    BigInt,        # 64-bit signed integer
    Decimal,       # Arbitrary precision decimal
    Currency,      # Currency amount (2 decimal places)

    # Identity types
    UUID,          # UUID v4 identifier
    ULID,          # ULID identifier
    Slug,          # URL-safe slug
    Email,         # Email address (validated)

    # Collections
    JSON,          # Arbitrary JSON object/array
    JSONArray,     # JSON array
    JSONObject,    # JSON object

    # Specialized types
    URL,           # HTTP/HTTPS URL
    IPv4,          # IPv4 address
    IPv6,          # IPv6 address
    PhoneNumber,   # E.164 phone number
    CountryCode,   # ISO 3166-1 alpha-2
    LanguageCode,  # ISO 639-1 language code
)

@fraiseql.type
class Contact:
    id: UUID
    name: str
    email: Email
    phone: PhoneNumber | None
    created_at: DateTime
    metadata: JSON
    balance: Decimal
```

Full scalar types list: See [Scalar Types Reference](../../reference/scalars.md)

---

## Schema Export & Compilation

### Exporting Schema

FraiseQL converts Python decorators to GraphQL schema JSON.

**Basic Export**:

```python
# In your main file or setup.py
import fraiseql

# Define your types, queries, mutations...

# Export schema
if __name__ == "__main__":
    fraiseql.export_schema("schema.json")
```

**Command-line Export**:

```bash
# Python module
python -m fraiseql export schema.json

# Or with specific module
python -m fraiseql export --module myproject.schema schema.json
```

**Programmatic Export**:

```python
from fraiseql import Exporter

exporter = Exporter()
schema_json = exporter.export_to_string()
print(schema_json)

# Or to file
exporter.export_to_file("schema.json")
```

### Configuration via TOML

Configuration flows from TOML through the compiler to the runtime.

**fraiseql.toml**:

```toml
# Security configuration
[fraiseql.security]
requires_auth = true
default_role = "user"

# Rate limiting
[fraiseql.security.rate_limiting]
enabled = true
auth_start_max_requests = 100
auth_start_window_secs = 60
authenticated_max_requests = 1000
authenticated_window_secs = 60

# Audit logging
[fraiseql.security.audit_logging]
enabled = true
log_level = "info"

# CORS
[fraiseql.security.cors]
allowed_origins = ["https://example.com"]
allowed_methods = ["GET", "POST"]
allowed_headers = ["Content-Type", "Authorization"]

# Database
[fraiseql.database]
pool_size = 10
connection_timeout = 30
statement_cache_size = 100

# Caching
[fraiseql.cache]
enabled = true
default_ttl = 300

# Observability
[fraiseql.observability]
trace_sampling_rate = 0.1
log_level = "info"
```

### Compilation Workflow

```bash
# 1. Export schema from Python
python schema.py  # Generates schema.json

# 2. Compile with configuration
fraiseql-cli compile schema.json fraiseql.toml

# 3. Deploy compiled schema
fraiseql-server --schema schema.compiled.json --config fraiseql.toml
```

**Output**: `schema.compiled.json` (types + queries + SQL + configuration)

---

## Type Mapping Reference

### Scalar Type Mapping

| Python Type | GraphQL Type | SQL Type (PostgreSQL) | Example |
|-------------|-------------|----------------------|---------|
| `int` | `Int` | `INTEGER` | `42` |
| `float` | `Float` | `DECIMAL` | `3.14` |
| `str` | `String` | `TEXT` | `"hello"` |
| `bool` | `Boolean` | `BOOLEAN` | `True` |
| `UUID` | `String` | `UUID` | `"550e8400..."` |
| `DateTime` | `String` | `TIMESTAMPTZ` | `"2026-02-05T..."` |
| `Date` | `String` | `DATE` | `"2026-02-05"` |
| `Decimal` | `String` | `NUMERIC` | `"123.45"` |
| `JSON` | `String` | `JSONB` | `'{"key": "val"}'` |

### Nullability Mapping

| Python | GraphQL | Meaning |
|--------|---------|---------|
| `int` | `Int!` | Required, non-null |
| `int \| None` | `Int` | Optional, nullable |
| `list[int]` | `[Int!]!` | Required non-null list of non-null ints |
| `list[int \| None]` | `[Int]!` | Required non-null list of nullable ints |
| `list[int] \| None` | `[Int!]` | Optional nullable list of non-null ints |

### Container Type Mapping

| Python | GraphQL | Notes |
|--------|---------|-------|
| `list[T]` | `[T!]!` | Non-empty list |
| `list[T \| None]` | `[T]!` | List with nullable elements |
| `dict[str, str]` | `JSON` | Use JSON scalar |
| `dict[str, Any]` | `JSON` | Use JSON scalar |

---

## Common Patterns

### 1. CRUD Operations

Complete create, read, update, delete pattern:

```python
import fraiseql
from fraiseql.scalars import UUID, DateTime

@fraiseql.type
class Todo:
    """A todo item."""
    id: UUID
    title: str
    description: str | None
    completed: bool
    created_at: DateTime
    updated_at: DateTime

# CREATE - Insert new
@fraiseql.mutation(sql_source="fn_create_todo", operation="CREATE")
def create_todo(title: str, description: str | None = None) -> Todo:
    """Create a new todo item."""
    pass

# READ - Get by ID
@fraiseql.query(sql_source="v_todo_by_id")
def todo(id: UUID) -> Todo | None:
    """Get a todo by ID."""
    pass

# READ - List all
@fraiseql.query(sql_source="v_todos")
def todos(
    limit: int = 50,
    offset: int = 0,
    completed: bool | None = None
) -> list[Todo]:
    """List todos with optional filtering."""
    pass

# UPDATE - Modify existing
@fraiseql.mutation(sql_source="fn_update_todo", operation="UPDATE")
def update_todo(
    id: UUID,
    title: str | None = None,
    description: str | None = None,
    completed: bool | None = None
) -> Todo:
    """Update a todo item."""
    pass

# DELETE - Remove
@fraiseql.mutation(sql_source="fn_delete_todo", operation="DELETE")
def delete_todo(id: UUID) -> bool:
    """Delete a todo item."""
    pass
```

### 2. Pagination Pattern

Implement cursor-based and offset-based pagination:

```python
@fraiseql.type
class PageInfo:
    """Pagination metadata."""
    has_next: bool
    has_previous: bool
    total_count: int
    page: int
    page_size: int

@fraiseql.type
class UserConnection:
    """Connection result with pagination."""
    items: list[User]
    page_info: PageInfo

# Offset-based pagination
@fraiseql.query(sql_source="v_users_paginated")
def users_paginated(
    limit: int = 20,
    offset: int = 0
) -> UserConnection:
    """Get users with pagination."""
    pass

# For cursor-based, use keyset pagination
@fraiseql.query(sql_source="v_users_keyset")
def users_keyset(
    first: int = 20,
    after: str | None = None
) -> UserConnection:
    """Get users using cursor-based pagination."""
    pass
```

### 3. Search & Filtering

Implement flexible search and filtering:

```python
@fraiseql.type
class SearchResult:
    """Search result with relevance."""
    item: User
    score: float

@fraiseql.query(sql_source="fn_search_users")
def search_users(
    query: str,
    filters: str | None = None,  # JSON filters
    limit: int = 20
) -> list[SearchResult]:
    """Full-text search users."""
    pass

@fraiseql.query(sql_source="v_users_advanced")
def users_advanced(
    name: str | None = None,
    email: str | None = None,
    created_after: str | None = None,
    created_before: str | None = None,
    is_active: bool | None = None
) -> list[User]:
    """Advanced user filtering."""
    pass
```

### 4. Multi-Tenant Pattern

Isolate data by tenant:

```python
@fraiseql.type
class TenantData:
    """Tenant-scoped data."""
    id: int
    tenant_id: UUID
    content: str

@fraiseql.query(sql_source="v_tenant_data")
@fraiseql.security(requires_auth=True, multi_tenant=True)
def my_data(limit: int = 50) -> list[TenantData]:
    """Get current tenant's data only (auto-filtered)."""
    pass

@fraiseql.mutation(
    sql_source="fn_create_tenant_data",
    operation="CREATE"
)
@fraiseql.security(requires_auth=True, multi_tenant=True)
def create_data(content: str) -> TenantData:
    """Create data in current tenant (tenant_id auto-injected)."""
    pass
```

### 5. Analytics Pattern

Define dimension and measure structures:

```python
from fraiseql.scalars import DateTime, Decimal

@fraiseql.fact_table(
    table_name="tf_metrics",
    measures=["value", "count"],
    dimension_paths=[
        {"name": "region", "json_path": "loc->>'region'", "data_type": "text"},
        {"name": "service", "json_path": "loc->>'service'", "data_type": "text"},
    ]
)
@fraiseql.type
class Metric:
    id: int
    value: Decimal
    count: int
    recorded_at: DateTime

@fraiseql.aggregate_query(
    fact_table="tf_metrics",
    auto_group_by=True,
    auto_aggregates=True
)
@fraiseql.query
def metrics_by_region(
    start_date: str | None = None,
    end_date: str | None = None
) -> list[dict]:
    """Metrics aggregated by region and service."""
    pass
```

---

## Error Handling

### Exception Types

FraiseQL raises specific exceptions:

```python
from fraiseql import (
    FraiseQLError,           # Base exception
    ValidationError,         # Schema validation failed
    CompilationError,        # Schema compilation failed
    ExportError,            # Schema export failed
    TypeError,              # Type annotation invalid
    DuplicateDefinitionError, # Name already defined
)

try:
    fraiseql.export_schema("schema.json")
except ValidationError as e:
    print(f"Validation failed: {e.message}")
except CompilationError as e:
    print(f"Compilation failed: {e.message}")
except FraiseQLError as e:
    print(f"FraiseQL error: {e.message}")
```

### Type Annotation Errors

Common type annotation issues:

```python
# ❌ Missing type annotation
@fraiseql.type
class BadType:
    id  # ERROR: Missing type annotation

# ❌ Invalid type reference
@fraiseql.query(sql_source="v_data")
def query1() -> UndefinedType:  # ERROR: Type not decorated
    pass

# ✅ Correct
@fraiseql.type
class GoodType:
    id: int

@fraiseql.query(sql_source="v_data")
def query1() -> GoodType:
    pass
```

### Field Validation Errors

At compile-time, FraiseQL validates:

- All types are decorated
- All fields have annotations
- All `@query` and `@mutation` return a valid type
- No circular references
- Parameter types match field types

---

## Testing

### Unit Test Pattern

Test schema structure:

```python
# tests/test_schema.py
import pytest
import fraiseql
from myapp.schema import User, Post, create_user

def test_user_type_defined():
    """User type should be properly defined."""
    assert User is not None

def test_create_user_mutation_exists():
    """create_user mutation should be callable."""
    assert create_user is not None

def test_schema_exports():
    """Schema should export without errors."""
    schema_json = fraiseql.export_to_string()
    assert "User" in schema_json
    assert "createUser" in schema_json
```

### Schema Validation Test

```python
# tests/test_schema_validation.py
import json
import fraiseql

def test_schema_valid_json():
    """Exported schema should be valid JSON."""
    schema_str = fraiseql.export_to_string()
    schema = json.loads(schema_str)
    assert "types" in schema
    assert "queries" in schema

def test_type_mapping():
    """Types should map correctly to GraphQL."""
    schema_str = fraiseql.export_to_string()
    schema = json.loads(schema_str)

    user_type = next(t for t in schema["types"] if t["name"] == "User")
    assert user_type["fields"]["id"]["type"] == "Int!"
    assert user_type["fields"]["email"]["type"] == "String"
```

### Schema Compilation Test

```python
# tests/test_compilation.py
import subprocess
import json

def test_schema_compiles():
    """Schema should compile successfully."""
    # Export
    fraiseql.export_schema("test_schema.json")

    # Compile
    result = subprocess.run(
        ["fraiseql-cli", "compile", "test_schema.json"],
        capture_output=True
    )

    assert result.returncode == 0

    # Verify compiled schema
    with open("schema.compiled.json") as f:
        compiled = json.load(f)
    assert "queries" in compiled
    assert "mutations" in compiled
```

---

## Best Practices

### Type Definition

1. **Use descriptive names**: `User` not `U`
2. **Add docstrings**: They become GraphQL descriptions
3. **Keep flat**: Avoid deep nesting (2-3 levels max)
4. **Be explicit**: `User | None` not implicit nullability

```python
@fraiseql.type
class User:
    """A user account in the system.

    Represents a registered user with identity and contact info.
    """
    id: int
    email: str
    name: str
    bio: str | None
```

### Query Definition

1. **Name queries for their action**: `get_user` not `user_info`
2. **Provide defaults**: Makes GraphQL arguments optional
3. **Limit result sets**: Always provide pagination
4. **Map to SQL views**: Use `@fraiseql.query` with `sql_source`

```python
@fraiseql.query(sql_source="v_users")
def users(limit: int = 20, offset: int = 0) -> list[User]:
    """Get paginated list of users."""
    pass
```

### Mutation Definition

1. **Clear operation type**: Use `operation="CREATE|UPDATE|DELETE"`
2. **Return result**: Always return affected record/status
3. **Validate inputs**: SQL functions should validate
4. **Handle optionals**: Use `T | None` for optional updates

```python
@fraiseql.mutation(sql_source="fn_update_user", operation="UPDATE")
def update_user(
    id: int,
    email: str | None = None
) -> User:
    """Update user email (null values are ignored)."""
    pass
```

### Performance

1. **Use fact tables for analytics**: Not operational queries
2. **Index denormalized columns**: For fast filtering
3. **Cache read-heavy queries**: Use `cache_ttl` parameter
4. **Batch mutations**: Use `list[T]` for bulk operations

```python
@fraiseql.query(sql_source="v_trending", cache_ttl=300)
def trending(limit: int = 10) -> list[Item]:
    """Trending items cached for 5 minutes."""
    pass
```

### Security

1. **Require auth for sensitive operations**: Use `@fraiseql.security`
2. **Field-level access**: Hide sensitive fields from non-admin
3. **Validate at database**: SQL functions should enforce rules
4. **Log access**: Use audit logging decorators

```python
@fraiseql.query(sql_source="v_user")
@fraiseql.security(
    requires_auth=True,
    field_level={"ssn": ["admin"]}
)
def user(id: int) -> User | None:
    """User with SSN visible to admin only."""
    pass
```

---

## Known Limitations

### Current Constraints

- ❌ **No custom resolvers**: All operations must map to SQL
- ❌ **No directives**: GraphQL directives not supported
- ❌ **No union types**: Only concrete `@fraiseql.type` classes
- ❌ **No interfaces**: Types are independent
- ❌ **No input types**: Use scalars for arguments
- ❌ **No circular references**: A → B → A not allowed
- ❌ **No inheritance**: Extend by composition, not inheritance
- ❌ **No polymorphism**: One concrete type per definition

### Workarounds

```python
# Union types - Use discriminator field
@fraiseql.type
class Result:
    status: str  # "user" | "error"
    user: User | None
    error_message: str | None

# Interfaces - Use composition
@fraiseql.type
class TimestampedData:
    created_at: str
    updated_at: str

@fraiseql.type
class User:
    id: int
    created_at: str  # Redundant, but necessary without interfaces
    updated_at: str

# Input validation - Use SQL functions
@fraiseql.mutation(sql_source="fn_create_validated_user", operation="CREATE")
def create_user(name: str) -> User:
    """SQL function validates name length."""
    pass
```

---

## See Also

- **Architecture Guide**: [FraiseQL Architecture Principles](../../guides/ARCHITECTURE_PRINCIPLES.md)
- **GraphQL Scalar Types**: [60+ Scalar Type Reference](../../reference/scalars.md)
- **Analytics Guide**: [Fact Tables & OLAP](../../guides/analytics-olap.md)
- **Security Guide**: [RBAC & Authorization](../../guides/security-and-rbac.md)
- **Database Patterns**: [SQL View & Function Patterns](../../guides/database-patterns.md)
- **Other SDKs**: [TypeScript](./typescript-reference.md), [Go](./go-reference.md), [Java](./java-reference.md)

---

## Getting Help

- **Issues**: [GitHub Issues](https://github.com/fraiseql/fraiseql/issues)
- **Discussions**: [GitHub Discussions](https://github.com/fraiseql/fraiseql/discussions)
- **Stack Overflow**: Tag with `fraiseql`
- **Community**: [Discord](https://discord.gg/fraiseql)

---

**Status**: ✅ Production Ready
**Last Updated**: 2026-02-05
**Maintained By**: FraiseQL Community
**License**: MIT
