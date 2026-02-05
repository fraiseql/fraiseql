<!-- Skip to main content -->
---
title: FraiseQL Python SDK Reference
description: Complete API reference for the FraiseQL Python SDK. This guide covers the complete Python authoring interface for building type-safe GraphQL APIs with Python de
keywords: ["framework", "directives", "types", "sdk", "schema", "scalars", "monitoring", "api"]
tags: ["documentation", "reference"]
---

# FraiseQL Python SDK Reference

**Status**: Production-Ready | **Python Version**: 3.10+ | **SDK Version**: 2.0.0+
**Last Updated**: 2026-02-05 | **Maintained By**: FraiseQL Community

Complete API reference for the FraiseQL Python SDK. This guide covers the complete Python authoring interface for building type-safe GraphQL APIs with Python decorators and type hints.

## Quick Start

```bash
<!-- Code example in BASH -->
# Installation
pip install FraiseQL

# Or with uv (recommended)
uv add FraiseQL
```text
<!-- Code example in TEXT -->

**Requirements**:

- Python 3.10 or later
- Type annotations (PEP 484, 586, 604)
- pip or uv package manager

**First Schema** (30 seconds):

```python
<!-- Code example in Python -->
import FraiseQL

@FraiseQL.type
class User:
    id: int
    name: str

@FraiseQL.query(sql_source="v_users")
def users(limit: int = 10) -> list[User]:
    """Get all users."""
    pass

FraiseQL.export_schema("schema.json")
```text
<!-- Code example in TEXT -->

Export and deploy to your FraiseQL server:

```bash
<!-- Code example in BASH -->
FraiseQL-cli compile schema.json FraiseQL.toml
FraiseQL-server --schema schema.compiled.json
```text
<!-- Code example in TEXT -->

---

## Quick Reference Table

| Feature | Decorator | Purpose | Returns |
|---------|-----------|---------|---------|
| **Types** | `@FraiseQL.type` | GraphQL object types | JSON schema |
| **Queries** | `@FraiseQL.query()` | Read operations (SELECT) | Single or list |
| **Mutations** | `@FraiseQL.mutation()` | Write operations (INSERT/UPDATE/DELETE) | Type result |
| **Fact Tables** | `@FraiseQL.fact_table()` | Analytics tables (OLAP) | Aggregation schema |
| **Aggregate Queries** | `@FraiseQL.aggregate_query()` | Analytics queries | Aggregated results |
| **Observers** | `@FraiseQL.observer()` | Event webhooks (async) | Event response |
| **Security** | `@FraiseQL.security()` | RBAC and access control | Auth metadata |
| **Subscriptions** | `@FraiseQL.subscription()` | Real-time pub/sub | Event stream |
| **Validators** | `@FraiseQL.validator()` | Field validation | Validation result |

---

## Type System

### 1. The `@FraiseQL.type` Decorator

Define GraphQL object types using Python classes with type annotations.

**Signature:**

```python
<!-- Code example in Python -->
@FraiseQL.type
class MyType:
    field1: int
    field2: str
    field3: bool
```text
<!-- Code example in TEXT -->

**Key Features**:

- **Type Annotations Required**: All fields must have Python type hints
- **Nullability**: Use `T | None` to indicate optional fields (Python 3.10+)
- **Nested Types**: Reference other `@FraiseQL.type` classes
- **Lists**: Use `list[T]` for array types
- **Docstrings**: Become GraphQL type descriptions
- **No Inheritance**: Each type is independent (flat schema)

**Examples**:

```python
<!-- Code example in Python -->
# ✅ Simple type
@FraiseQL.type
class User:
    """A user account."""
    id: int
    username: str
    email: str

# ✅ With nullable fields
@FraiseQL.type
class Post:
    """A blog post."""
    id: int
    title: str
    body: str
    published_at: str | None  # Optional field

# ✅ With lists
@FraiseQL.type
class Blog:
    """A blog with multiple posts."""
    id: int
    name: str
    posts: list[Post]
    tags: list[str]

# ✅ Nested types
@FraiseQL.type
class Address:
    """A physical address."""
    street: str
    city: str
    state: str
    postal_code: str

@FraiseQL.type
class Company:
    """A company with address."""
    id: int
    name: str
    headquarters: Address
    employees: list[User]

# ✅ With docstrings for GraphQL descriptions
@FraiseQL.type
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
```text
<!-- Code example in TEXT -->

**Advanced Type Features**:

```python
<!-- Code example in Python -->
# Nullable list elements
@FraiseQL.type
class UserSearchResult:
    """Results with potential nulls."""
    matches: list[User | None]  # List can contain nulls

# Complex nested structure
@FraiseQL.type
class Department:
    """Represents a department."""
    id: int
    name: str
    manager: User | None
    members: list[User]
    budget: float
    created_at: str

# Multiple levels of nesting
@FraiseQL.type
class Organization:
    """An organization with multiple departments."""
    id: int
    name: str
    departments: list[Department]
```text
<!-- Code example in TEXT -->

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
| `@FraiseQL.type class User` | `User!` | Custom object type (non-null) |
| `User \| None` | `User` | Nullable custom type |
| `list[User]` | `[User!]!` | Non-empty list of users |
| `list[User \| None]` | `[User]!` | List with nullable users |

**Scalar Type Extensions** (60+ available):

```python
<!-- Code example in Python -->
from FraiseQL.scalars import (
    DateTime,  # ISO 8601 datetime
    Date,      # ISO 8601 date
    Time,      # ISO 8601 time
    UUID,      # UUID v4
    JSON,      # Arbitrary JSON
    BigInt,    # 64-bit integer
    Decimal,   # Arbitrary precision
)

@FraiseQL.type
class Event:
    id: UUID
    occurred_at: DateTime
    metadata: JSON | None
    amount: Decimal
```text
<!-- Code example in TEXT -->

### 3. Modern Python 3.10+ Type Hints

FraiseQL requires modern type hint syntax:

```python
<!-- Code example in Python -->
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
```text
<!-- Code example in TEXT -->

---

## Operations

### 1. Queries: Read Operations

Queries are read-only operations that fetch data from views.

**Signature:**

```python
<!-- Code example in Python -->
@FraiseQL.query(sql_source="view_name")
def query_name(arg1: int, arg2: str = "default") -> ResultType:
    """Query description."""
    pass
```text
<!-- Code example in TEXT -->

**Parameters**:

- `sql_source` (optional): SQL view or function name to execute
- `auto_params` (optional): Dictionary of parameter configurations
- `cache_ttl` (optional): Cache results for N seconds (0 = no cache)

**Examples**:

```python
<!-- Code example in Python -->
# Simple list query
@FraiseQL.query(sql_source="v_users")
def users(limit: int = 10) -> list[User]:
    """Get all users with pagination."""
    pass

# Single result query
@FraiseQL.query(sql_source="v_user_by_id")
def user(id: int) -> User | None:
    """Get a user by ID, returns null if not found."""
    pass

# Query with multiple parameters
@FraiseQL.query(sql_source="v_search_users")
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
@FraiseQL.query(
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
@FraiseQL.query
def server_time() -> str:
    """Get current server time."""
    pass

# Cached query (results cached for 300 seconds)
@FraiseQL.query(sql_source="v_trending", cache_ttl=300)
def trending_items(limit: int = 10) -> list[Item]:
    """Get trending items (cached for 5 minutes)."""
    pass
```text
<!-- Code example in TEXT -->

**GraphQL Generated**:

```graphql
<!-- Code example in GraphQL -->
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
```text
<!-- Code example in TEXT -->

**Query Argument Handling**:

Arguments follow Python function signature conventions:

```python
<!-- Code example in Python -->
@FraiseQL.query(sql_source="v_data")
def get_data(
    required_arg: int,           # Required (no default)
    optional_arg: str = "default",  # Optional (has default)
    nullable_arg: int | None = None # Nullable + optional
) -> list[dict]:
    """Demonstrates all argument types."""
    pass
```text
<!-- Code example in TEXT -->

Generates:

```graphql
<!-- Code example in GraphQL -->
type Query {
  getData(
    requiredArg: Int!
    optionalArg: String = "default"
    nullableArg: Int
  ): [dict!]!
}
```text
<!-- Code example in TEXT -->

### 2. Mutations: Write Operations

Mutations are write operations that modify data (CREATE, UPDATE, DELETE).

**Signature:**

```python
<!-- Code example in Python -->
@FraiseQL.mutation(
    sql_source="function_name",
    operation="CREATE"  # CREATE | UPDATE | DELETE | CUSTOM
)
def mutation_name(arg1: str, arg2: int) -> ResultType:
    """Mutation description."""
    pass
```text
<!-- Code example in TEXT -->

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
<!-- Code example in Python -->
# Create mutation
@FraiseQL.mutation(
    sql_source="fn_create_user",
    operation="CREATE"
)
def create_user(name: str, email: str) -> User:
    """Create a new user account."""
    pass

# Update mutation
@FraiseQL.mutation(
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
@FraiseQL.mutation(
    sql_source="fn_delete_user",
    operation="DELETE"
)
def delete_user(id: int) -> bool:
    """Delete a user and return success status."""
    pass

# Batch operation
@FraiseQL.mutation(
    sql_source="fn_bulk_update_users",
    operation="UPDATE"
)
def bulk_update_users(ids: list[int], status: str) -> list[User]:
    """Update multiple users' status at once."""
    pass

# Complex mutation with nested result
@FraiseQL.mutation(
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
@FraiseQL.mutation(
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
```text
<!-- Code example in TEXT -->

**GraphQL Generated**:

```graphql
<!-- Code example in GraphQL -->
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
```text
<!-- Code example in TEXT -->

### 3. Subscriptions: Real-time Events

Subscriptions provide real-time data via WebSocket or Server-Sent Events.

**Signature:**

```python
<!-- Code example in Python -->
@FraiseQL.subscription(
    topic="channel_name",
    message_type=MessageType
)
def subscription_name(filter_arg: str | None = None) -> MessageType:
    """Subscription description."""
    pass
```text
<!-- Code example in TEXT -->

**Examples**:

```python
<!-- Code example in Python -->
@FraiseQL.type
class UserCreatedEvent:
    """Fired when a new user is created."""
    user: User
    created_at: str

@FraiseQL.subscription(
    topic="users.created",
    message_type=UserCreatedEvent
)
def on_user_created() -> UserCreatedEvent:
    """Subscribe to new user creation events."""
    pass

@FraiseQL.subscription(
    topic="users.updated",
    message_type=User
)
def on_user_updated(user_id: int) -> User:
    """Subscribe to updates for a specific user."""
    pass

@FraiseQL.subscription(
    topic="messages",
    message_type=Message
)
def messages(room_id: int | None = None) -> Message:
    """Subscribe to new messages, optionally filtered by room."""
    pass
```text
<!-- Code example in TEXT -->

---

## Advanced Features

### 1. Fact Tables: Analytics

Define analytics tables for OLAP queries.

**Signature:**

```python
<!-- Code example in Python -->
@FraiseQL.fact_table(
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
@FraiseQL.type
class Sale:
    id: int
    revenue: float
    quantity: int
```text
<!-- Code example in TEXT -->

**Parameters**:

- `table_name` (required): SQL table name (must start with `tf_`)
- `measures` (required): List of numeric column names
- `dimension_column` (optional): JSONB column name (default: "data")
- `dimension_paths` (optional): Dimension definitions
- `denormalized_columns` (optional): Fast-access filter columns

**Examples**:

```python
<!-- Code example in Python -->
# Multi-dimensional fact table
@FraiseQL.fact_table(
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
@FraiseQL.type
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
@FraiseQL.fact_table(
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
@FraiseQL.type
class Event:
    """An analytics event."""
    id: int
    count: int
    duration: float
    user_id: int
    occurred_at: str
```text
<!-- Code example in TEXT -->

**SQL Table Pattern**:

```sql
<!-- Code example in SQL -->
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
```text
<!-- Code example in TEXT -->

### 2. Aggregate Queries: OLAP Analytics

Define flexible analytics queries on fact tables.

**Signature:**

```python
<!-- Code example in Python -->
@FraiseQL.aggregate_query(
    fact_table="tf_sales",
    auto_group_by=True,
    auto_aggregates=True
)
@FraiseQL.query
def analytics_query() -> list[dict]:
    """Analytics query description."""
    pass
```text
<!-- Code example in TEXT -->

**Parameters**:

- `fact_table` (required): Fact table name (from `@FraiseQL.fact_table`)
- `auto_group_by` (optional): Auto-generate GROUP BY fields (default: True)
- `auto_aggregates` (optional): Auto-generate aggregates (default: True)
- `allow_empty_group_by` (optional): Allow queries without grouping (default: False)

**Examples**:

```python
<!-- Code example in Python -->
# Sales by category
@FraiseQL.aggregate_query(
    fact_table="tf_sales",
    auto_group_by=True,
    auto_aggregates=True
)
@FraiseQL.query
def sales_by_category(
    start_date: str | None = None,
    end_date: str | None = None,
    limit: int = 100
) -> list[dict]:
    """Sales aggregated by category and time."""
    pass

# Custom aggregation (manual configuration)
@FraiseQL.aggregate_query(
    fact_table="tf_sales",
    auto_group_by=False,
    auto_aggregates=False
)
@FraiseQL.query
def custom_sales_analysis() -> list[dict]:
    """Fully custom sales aggregation."""
    pass

# Revenue analysis with filtering
@FraiseQL.aggregate_query(
    fact_table="tf_sales"
)
@FraiseQL.query
def revenue_analysis(
    min_revenue: float = 0,
    region: str | None = None
) -> list[dict]:
    """Analyze revenue by multiple dimensions."""
    pass
```text
<!-- Code example in TEXT -->

**Generated GraphQL**:

```graphql
<!-- Code example in GraphQL -->
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
```text
<!-- Code example in TEXT -->

### 3. Observers: Event Webhooks

Observers trigger async webhooks when events occur.

**Signature:**

```python
<!-- Code example in Python -->
@FraiseQL.observer(
    on="mutation_name",
    trigger="success"  # success | failure | always
)
def observer_name(event: ObserverEvent) -> bool:
    """Observer description."""
    pass
```text
<!-- Code example in TEXT -->

**Examples**:

```python
<!-- Code example in Python -->
@FraiseQL.type
class UserCreatedEvent:
    """Event fired when a user is created."""
    user: User
    timestamp: str

# Send webhook after user creation
@FraiseQL.observer(
    on="create_user",
    trigger="success"
)
def notify_on_user_created(event: UserCreatedEvent) -> bool:
    """Send notification when user is created."""
    # This gets compiled to call webhooks
    pass

# Log all user updates
@FraiseQL.observer(
    on="update_user",
    trigger="always"
)
def log_user_update(event: dict) -> bool:
    """Log all user update attempts."""
    pass
```text
<!-- Code example in TEXT -->

### 4. Security & RBAC

Control access using role-based access control.

**Signature:**

```python
<!-- Code example in Python -->
@FraiseQL.security(
    requires_auth=True,
    roles=["admin", "user"],
    field_level={"sensitive_field": ["admin"]}
)
def operation_name() -> ResultType:
    pass
```text
<!-- Code example in TEXT -->

**Examples**:

```python
<!-- Code example in Python -->
# Public query (no auth required)
@FraiseQL.query(sql_source="v_public_data")
@FraiseQL.security(requires_auth=False)
def public_data(limit: int = 10) -> list[PublicData]:
    """Publicly accessible data."""
    pass

# Admin-only query
@FraiseQL.query(sql_source="v_admin_stats")
@FraiseQL.security(requires_auth=True, roles=["admin"])
def admin_stats() -> dict:
    """Administrative statistics (admin only)."""
    pass

# User query with field-level security
@FraiseQL.type
class UserProfile:
    id: int
    name: str
    email: str
    ssn: str  # Sensitive field

@FraiseQL.query(sql_source="v_user_profile")
@FraiseQL.security(
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
@FraiseQL.query(sql_source="v_tenant_data")
@FraiseQL.security(requires_auth=True, multi_tenant=True)
def my_data(limit: int = 10) -> list[TenantData]:
    """Get only current tenant's data."""
    pass
```text
<!-- Code example in TEXT -->

---

## Scalar Types Reference

FraiseQL supports 60+ scalar types. Common examples:

```python
<!-- Code example in Python -->
from FraiseQL.scalars import (
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

@FraiseQL.type
class Contact:
    id: UUID
    name: str
    email: Email
    phone: PhoneNumber | None
    created_at: DateTime
    metadata: JSON
    balance: Decimal
```text
<!-- Code example in TEXT -->

Full scalar types list: See [Scalar Types Reference](../../reference/scalars.md)

---

## Schema Export & Compilation

### Exporting Schema

FraiseQL converts Python decorators to GraphQL schema JSON.

**Basic Export**:

```python
<!-- Code example in Python -->
# In your main file or setup.py
import FraiseQL

# Define your types, queries, mutations...

# Export schema
if __name__ == "__main__":
    FraiseQL.export_schema("schema.json")
```text
<!-- Code example in TEXT -->

**Command-line Export**:

```bash
<!-- Code example in BASH -->
# Python module
python -m FraiseQL export schema.json

# Or with specific module
python -m FraiseQL export --module myproject.schema schema.json
```text
<!-- Code example in TEXT -->

**Programmatic Export**:

```python
<!-- Code example in Python -->
from FraiseQL import Exporter

exporter = Exporter()
schema_json = exporter.export_to_string()
print(schema_json)

# Or to file
exporter.export_to_file("schema.json")
```text
<!-- Code example in TEXT -->

### Configuration via TOML

Configuration flows from TOML through the compiler to the runtime.

**FraiseQL.toml**:

```toml
<!-- Code example in TOML -->
# Security configuration
[FraiseQL.security]
requires_auth = true
default_role = "user"

# Rate limiting
[FraiseQL.security.rate_limiting]
enabled = true
auth_start_max_requests = 100
auth_start_window_secs = 60
authenticated_max_requests = 1000
authenticated_window_secs = 60

# Audit logging
[FraiseQL.security.audit_logging]
enabled = true
log_level = "info"

# CORS
[FraiseQL.security.cors]
allowed_origins = ["https://example.com"]
allowed_methods = ["GET", "POST"]
allowed_headers = ["Content-Type", "Authorization"]

# Database
[FraiseQL.database]
pool_size = 10
connection_timeout = 30
statement_cache_size = 100

# Caching
[FraiseQL.cache]
enabled = true
default_ttl = 300

# Observability
[FraiseQL.observability]
trace_sampling_rate = 0.1
log_level = "info"
```text
<!-- Code example in TEXT -->

### Compilation Workflow

```bash
<!-- Code example in BASH -->
# 1. Export schema from Python
python schema.py  # Generates schema.json

# 2. Compile with configuration
FraiseQL-cli compile schema.json FraiseQL.toml

# 3. Deploy compiled schema
FraiseQL-server --schema schema.compiled.json --config FraiseQL.toml
```text
<!-- Code example in TEXT -->

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
<!-- Code example in Python -->
import FraiseQL
from FraiseQL.scalars import UUID, DateTime

@FraiseQL.type
class Todo:
    """A todo item."""
    id: UUID
    title: str
    description: str | None
    completed: bool
    created_at: DateTime
    updated_at: DateTime

# CREATE - Insert new
@FraiseQL.mutation(sql_source="fn_create_todo", operation="CREATE")
def create_todo(title: str, description: str | None = None) -> Todo:
    """Create a new todo item."""
    pass

# READ - Get by ID
@FraiseQL.query(sql_source="v_todo_by_id")
def todo(id: UUID) -> Todo | None:
    """Get a todo by ID."""
    pass

# READ - List all
@FraiseQL.query(sql_source="v_todos")
def todos(
    limit: int = 50,
    offset: int = 0,
    completed: bool | None = None
) -> list[Todo]:
    """List todos with optional filtering."""
    pass

# UPDATE - Modify existing
@FraiseQL.mutation(sql_source="fn_update_todo", operation="UPDATE")
def update_todo(
    id: UUID,
    title: str | None = None,
    description: str | None = None,
    completed: bool | None = None
) -> Todo:
    """Update a todo item."""
    pass

# DELETE - Remove
@FraiseQL.mutation(sql_source="fn_delete_todo", operation="DELETE")
def delete_todo(id: UUID) -> bool:
    """Delete a todo item."""
    pass
```text
<!-- Code example in TEXT -->

### 2. Pagination Pattern

Implement cursor-based and offset-based pagination:

```python
<!-- Code example in Python -->
@FraiseQL.type
class PageInfo:
    """Pagination metadata."""
    has_next: bool
    has_previous: bool
    total_count: int
    page: int
    page_size: int

@FraiseQL.type
class UserConnection:
    """Connection result with pagination."""
    items: list[User]
    page_info: PageInfo

# Offset-based pagination
@FraiseQL.query(sql_source="v_users_paginated")
def users_paginated(
    limit: int = 20,
    offset: int = 0
) -> UserConnection:
    """Get users with pagination."""
    pass

# For cursor-based, use keyset pagination
@FraiseQL.query(sql_source="v_users_keyset")
def users_keyset(
    first: int = 20,
    after: str | None = None
) -> UserConnection:
    """Get users using cursor-based pagination."""
    pass
```text
<!-- Code example in TEXT -->

### 3. Search & Filtering

Implement flexible search and filtering:

```python
<!-- Code example in Python -->
@FraiseQL.type
class SearchResult:
    """Search result with relevance."""
    item: User
    score: float

@FraiseQL.query(sql_source="fn_search_users")
def search_users(
    query: str,
    filters: str | None = None,  # JSON filters
    limit: int = 20
) -> list[SearchResult]:
    """Full-text search users."""
    pass

@FraiseQL.query(sql_source="v_users_advanced")
def users_advanced(
    name: str | None = None,
    email: str | None = None,
    created_after: str | None = None,
    created_before: str | None = None,
    is_active: bool | None = None
) -> list[User]:
    """Advanced user filtering."""
    pass
```text
<!-- Code example in TEXT -->

### 4. Multi-Tenant Pattern

Isolate data by tenant:

```python
<!-- Code example in Python -->
@FraiseQL.type
class TenantData:
    """Tenant-scoped data."""
    id: int
    tenant_id: UUID
    content: str

@FraiseQL.query(sql_source="v_tenant_data")
@FraiseQL.security(requires_auth=True, multi_tenant=True)
def my_data(limit: int = 50) -> list[TenantData]:
    """Get current tenant's data only (auto-filtered)."""
    pass

@FraiseQL.mutation(
    sql_source="fn_create_tenant_data",
    operation="CREATE"
)
@FraiseQL.security(requires_auth=True, multi_tenant=True)
def create_data(content: str) -> TenantData:
    """Create data in current tenant (tenant_id auto-injected)."""
    pass
```text
<!-- Code example in TEXT -->

### 5. Analytics Pattern

Define dimension and measure structures:

```python
<!-- Code example in Python -->
from FraiseQL.scalars import DateTime, Decimal

@FraiseQL.fact_table(
    table_name="tf_metrics",
    measures=["value", "count"],
    dimension_paths=[
        {"name": "region", "json_path": "loc->>'region'", "data_type": "text"},
        {"name": "service", "json_path": "loc->>'service'", "data_type": "text"},
    ]
)
@FraiseQL.type
class Metric:
    id: int
    value: Decimal
    count: int
    recorded_at: DateTime

@FraiseQL.aggregate_query(
    fact_table="tf_metrics",
    auto_group_by=True,
    auto_aggregates=True
)
@FraiseQL.query
def metrics_by_region(
    start_date: str | None = None,
    end_date: str | None = None
) -> list[dict]:
    """Metrics aggregated by region and service."""
    pass
```text
<!-- Code example in TEXT -->

---

## Error Handling

### Exception Types

FraiseQL raises specific exceptions:

```python
<!-- Code example in Python -->
from FraiseQL import (
    FraiseQLError,           # Base exception
    ValidationError,         # Schema validation failed
    CompilationError,        # Schema compilation failed
    ExportError,            # Schema export failed
    TypeError,              # Type annotation invalid
    DuplicateDefinitionError, # Name already defined
)

try:
    FraiseQL.export_schema("schema.json")
except ValidationError as e:
    print(f"Validation failed: {e.message}")
except CompilationError as e:
    print(f"Compilation failed: {e.message}")
except FraiseQLError as e:
    print(f"FraiseQL error: {e.message}")
```text
<!-- Code example in TEXT -->

### Type Annotation Errors

Common type annotation issues:

```python
<!-- Code example in Python -->
# ❌ Missing type annotation
@FraiseQL.type
class BadType:
    id  # ERROR: Missing type annotation

# ❌ Invalid type reference
@FraiseQL.query(sql_source="v_data")
def query1() -> UndefinedType:  # ERROR: Type not decorated
    pass

# ✅ Correct
@FraiseQL.type
class GoodType:
    id: int

@FraiseQL.query(sql_source="v_data")
def query1() -> GoodType:
    pass
```text
<!-- Code example in TEXT -->

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
<!-- Code example in Python -->
# tests/test_schema.py
import pytest
import FraiseQL
from myapp.schema import User, Post, create_user

def test_user_type_defined():
    """User type should be properly defined."""
    assert User is not None

def test_create_user_mutation_exists():
    """create_user mutation should be callable."""
    assert create_user is not None

def test_schema_exports():
    """Schema should export without errors."""
    schema_json = FraiseQL.export_to_string()
    assert "User" in schema_json
    assert "createUser" in schema_json
```text
<!-- Code example in TEXT -->

### Schema Validation Test

```python
<!-- Code example in Python -->
# tests/test_schema_validation.py
import json
import FraiseQL

def test_schema_valid_json():
    """Exported schema should be valid JSON."""
    schema_str = FraiseQL.export_to_string()
    schema = json.loads(schema_str)
    assert "types" in schema
    assert "queries" in schema

def test_type_mapping():
    """Types should map correctly to GraphQL."""
    schema_str = FraiseQL.export_to_string()
    schema = json.loads(schema_str)

    user_type = next(t for t in schema["types"] if t["name"] == "User")
    assert user_type["fields"]["id"]["type"] == "Int!"
    assert user_type["fields"]["email"]["type"] == "String"
```text
<!-- Code example in TEXT -->

### Schema Compilation Test

```python
<!-- Code example in Python -->
# tests/test_compilation.py
import subprocess
import json

def test_schema_compiles():
    """Schema should compile successfully."""
    # Export
    FraiseQL.export_schema("test_schema.json")

    # Compile
    result = subprocess.run(
        ["FraiseQL-cli", "compile", "test_schema.json"],
        capture_output=True
    )

    assert result.returncode == 0

    # Verify compiled schema
    with open("schema.compiled.json") as f:
        compiled = json.load(f)
    assert "queries" in compiled
    assert "mutations" in compiled
```text
<!-- Code example in TEXT -->

---

## Best Practices

### Type Definition

1. **Use descriptive names**: `User` not `U`
2. **Add docstrings**: They become GraphQL descriptions
3. **Keep flat**: Avoid deep nesting (2-3 levels max)
4. **Be explicit**: `User | None` not implicit nullability

```python
<!-- Code example in Python -->
@FraiseQL.type
class User:
    """A user account in the system.

    Represents a registered user with identity and contact info.
    """
    id: int
    email: str
    name: str
    bio: str | None
```text
<!-- Code example in TEXT -->

### Query Definition

1. **Name queries for their action**: `get_user` not `user_info`
2. **Provide defaults**: Makes GraphQL arguments optional
3. **Limit result sets**: Always provide pagination
4. **Map to SQL views**: Use `@FraiseQL.query` with `sql_source`

```python
<!-- Code example in Python -->
@FraiseQL.query(sql_source="v_users")
def users(limit: int = 20, offset: int = 0) -> list[User]:
    """Get paginated list of users."""
    pass
```text
<!-- Code example in TEXT -->

### Mutation Definition

1. **Clear operation type**: Use `operation="CREATE|UPDATE|DELETE"`
2. **Return result**: Always return affected record/status
3. **Validate inputs**: SQL functions should validate
4. **Handle optionals**: Use `T | None` for optional updates

```python
<!-- Code example in Python -->
@FraiseQL.mutation(sql_source="fn_update_user", operation="UPDATE")
def update_user(
    id: int,
    email: str | None = None
) -> User:
    """Update user email (null values are ignored)."""
    pass
```text
<!-- Code example in TEXT -->

### Performance

1. **Use fact tables for analytics**: Not operational queries
2. **Index denormalized columns**: For fast filtering
3. **Cache read-heavy queries**: Use `cache_ttl` parameter
4. **Batch mutations**: Use `list[T]` for bulk operations

```python
<!-- Code example in Python -->
@FraiseQL.query(sql_source="v_trending", cache_ttl=300)
def trending(limit: int = 10) -> list[Item]:
    """Trending items cached for 5 minutes."""
    pass
```text
<!-- Code example in TEXT -->

### Security

1. **Require auth for sensitive operations**: Use `@FraiseQL.security`
2. **Field-level access**: Hide sensitive fields from non-admin
3. **Validate at database**: SQL functions should enforce rules
4. **Log access**: Use audit logging decorators

```python
<!-- Code example in Python -->
@FraiseQL.query(sql_source="v_user")
@FraiseQL.security(
    requires_auth=True,
    field_level={"ssn": ["admin"]}
)
def user(id: int) -> User | None:
    """User with SSN visible to admin only."""
    pass
```text
<!-- Code example in TEXT -->

---

## Known Limitations

### Current Constraints

- ❌ **No custom resolvers**: All operations must map to SQL
- ❌ **No directives**: GraphQL directives not supported
- ❌ **No union types**: Only concrete `@FraiseQL.type` classes
- ❌ **No interfaces**: Types are independent
- ❌ **No input types**: Use scalars for arguments
- ❌ **No circular references**: A → B → A not allowed
- ❌ **No inheritance**: Extend by composition, not inheritance
- ❌ **No polymorphism**: One concrete type per definition

### Workarounds

```python
<!-- Code example in Python -->
# Union types - Use discriminator field
@FraiseQL.type
class Result:
    status: str  # "user" | "error"
    user: User | None
    error_message: str | None

# Interfaces - Use composition
@FraiseQL.type
class TimestampedData:
    created_at: str
    updated_at: str

@FraiseQL.type
class User:
    id: int
    created_at: str  # Redundant, but necessary without interfaces
    updated_at: str

# Input validation - Use SQL functions
@FraiseQL.mutation(sql_source="fn_create_validated_user", operation="CREATE")
def create_user(name: str) -> User:
    """SQL function validates name length."""
    pass
```text
<!-- Code example in TEXT -->

---

## See Also

- **Architecture Guide**: [FraiseQL Architecture Principles](../../architecture/README.md)
- **GraphQL Scalar Types**: [60+ Scalar Type Reference](../../reference/scalars.md)
- **Analytics Guide**: [Fact Tables & OLAP](../../guides/analytics-patterns.md)
- **Security Guide**: [RBAC & Authorization](../../guides/authorization-quick-start.md)
- **Database Patterns**: [SQL View & Function Patterns](../../guides/database-selection-guide.md)
- **Other SDKs**: [TypeScript](./typescript-reference.md), [Go](./go-reference.md), [Java](./java-reference.md)

---

## Getting Help

- **Issues**: [GitHub Issues](https://github.com/FraiseQL/FraiseQL/issues)
- **Discussions**: [GitHub Discussions](https://github.com/FraiseQL/FraiseQL/discussions)
- **Stack Overflow**: Tag with `FraiseQL`
- **Community**: [Discord](https://discord.gg/FraiseQL)

---

## Troubleshooting

### Common Setup Issues

#### Installation Problems

**Issue**: `ModuleNotFoundError: No module named 'FraiseQL'`

**Solutions**:

```bash
<!-- Code example in BASH -->
# Verify installation
python -m pip show FraiseQL

# Reinstall with upgrade
python -m pip install --upgrade FraiseQL

# Use uv (recommended)
uv sync
uv add FraiseQL

# Check Python version (3.10+ required)
python --version
```text
<!-- Code example in TEXT -->

**Debugging**:

- Verify you're in correct virtual environment: `which python`
- Check site-packages location: `python -c "import site; print(site.getsitepackages())"`
- Inspect pip cache: `pip cache info`

#### Import/Module Resolution Issues

**Issue**: `ImportError: cannot import name 'type' from 'FraiseQL'`

**Solutions**:

```python
<!-- Code example in Python -->
# ✅ Correct import style
from FraiseQL import type, query, mutation

# ❌ Incorrect
from FraiseQL.decorators import type  # This won't work
```text
<!-- Code example in TEXT -->

**Check version**:

```python
<!-- Code example in Python -->
import FraiseQL
print(FraiseQL.__version__)  # Should be 2.0.0+
```text
<!-- Code example in TEXT -->

#### Version Compatibility

**Issue**: `FraiseQL version 1.x installed, but code uses 2.x syntax`

**Check installed version**:

```bash
<!-- Code example in BASH -->
pip show FraiseQL | grep Version
```text
<!-- Code example in TEXT -->

**Upgrade to latest**:

```bash
<!-- Code example in BASH -->
pip install FraiseQL>=2.0.0
```text
<!-- Code example in TEXT -->

#### Dependency Conflicts

**Issue**: `pip install` fails with dependency resolution error

**Debug dependency tree**:

```bash
<!-- Code example in BASH -->
pip install pipdeptree
pipdeptree -p FraiseQL

# Check for conflicting versions
pip check
```text
<!-- Code example in TEXT -->

**Resolve manually**:

```bash
<!-- Code example in BASH -->
# Pin specific versions
pip install FraiseQL==2.0.0 pydantic>=2.0
```text
<!-- Code example in TEXT -->

---

### Type System Issues

#### Type Mismatch Errors

**Issue**: `ValidationError: field 'email' expects String, got UUID`

**Cause**: Python type annotation doesn't match decorator specification

**Solution**:

```python
<!-- Code example in Python -->
# ❌ Wrong - type annotation conflicts with decorator
@FraiseQL.type
class User:
    email: UUID  # But treating as string elsewhere

# ✅ Correct
from FraiseQL.scalars import Email

@FraiseQL.type
class User:
    email: Email  # Matches all usages
```text
<!-- Code example in TEXT -->

**Validate types before export**:

```python
<!-- Code example in Python -->
import FraiseQL
FraiseQL.validate_schema()  # Raises ValidationError if issues found
```text
<!-- Code example in TEXT -->

#### Nullability Problems

**Issue**: `GraphQL Error: User.email is non-null but received null`

**Cause**: Incorrect use of optional/non-null syntax

**Solution**:

```python
<!-- Code example in Python -->
# ❌ Wrong - implies non-null, but can return None
@FraiseQL.type
class User:
    email: str  # Non-null in GraphQL

# ✅ Correct - explicitly optional
@FraiseQL.type
class User:
    email: str | None  # Nullable in GraphQL
```text
<!-- Code example in TEXT -->

**Runtime null check**:

```python
<!-- Code example in Python -->
@FraiseQL.query(sql_source="v_users")
def user(id: int) -> User | None:  # Explicitly nullable
    """User may not be found."""
    pass
```text
<!-- Code example in TEXT -->

#### Generic Type Issues

**Issue**: `TypeError: 'list' is not subscriptable (Python <3.9)`

**Cause**: Using `list[T]` syntax without proper import

**Solution** (Python 3.10+):

```python
<!-- Code example in Python -->
# ✅ Works in Python 3.10+
def get_users() -> list[User]:
    pass
```text
<!-- Code example in TEXT -->

**Compatibility** (Python 3.9):

```python
<!-- Code example in Python -->
from typing import List
def get_users() -> List[User]:  # Use typing.List
    pass
```text
<!-- Code example in TEXT -->

**Always verify Python version**:

```python
<!-- Code example in Python -->
import sys
assert sys.version_info >= (3, 10), "FraiseQL requires Python 3.10+"
```text
<!-- Code example in TEXT -->

#### Schema Validation Errors

**Issue**: `ValidationError: Type 'UnknownType' is not defined`

**Cause**: Referencing non-existent type in return annotation

**Solution**:

```python
<!-- Code example in Python -->
# ❌ Wrong - UserType doesn't exist
@FraiseQL.query(sql_source="v_users")
def users() -> UserType:  # Not decorated with @FraiseQL.type
    pass

# ✅ Correct - Define the type first
@FraiseQL.type
class User:
    id: int
    name: str

@FraiseQL.query(sql_source="v_users")
def users() -> list[User]:
    pass
```text
<!-- Code example in TEXT -->

---

### Runtime Errors

#### Query Execution Failures

**Issue**: `FraiseQLError: Query execution failed: unknown table "v_users"`

**Cause**: SQL source table/view doesn't exist in database

**Debug**:

```python
<!-- Code example in Python -->
# Check if view exists
import psycopg2
conn = psycopg2.connect(os.getenv("DATABASE_URL"))
cur = conn.cursor()
cur.execute("""
    SELECT * FROM information_schema.views
    WHERE table_name = 'v_users'
""")
print(cur.fetchall())  # Should return 1 row
```text
<!-- Code example in TEXT -->

**Solution**:

```sql
<!-- Code example in SQL -->
-- Create missing view
CREATE VIEW v_users AS
SELECT id, name, email FROM users;
```text
<!-- Code example in TEXT -->

#### Connection Issues

**Issue**: `FraiseQLError: Failed to connect to database`

**Debug connection**:

```bash
<!-- Code example in BASH -->
# Test database connectivity
psql postgresql://user:pass@localhost/dbname -c "SELECT 1"

# Check environment variable
echo $DATABASE_URL
```text
<!-- Code example in TEXT -->

**Common causes**:

- Database not running: `docker ps | grep postgres`
- Wrong credentials: verify user/password
- Firewall blocking: check network connectivity
- Connection string format: `postgresql://user:pass@host:5432/db`

**Solution**:

```python
<!-- Code example in Python -->
import os

# Validate connection string
db_url = os.getenv("DATABASE_URL")
assert db_url, "DATABASE_URL not set"

# Test connection at startup
from FraiseQL import FraiseQLServer
try:
    server = FraiseQLServer.from_compiled("schema.compiled.json")
except Exception as e:
    print(f"Connection failed: {e}")
    raise
```text
<!-- Code example in TEXT -->

#### Timeout Problems

**Issue**: `TimeoutError: Query execution exceeded 30s timeout`

**Cause**: Complex query or slow database

**Debug**:

```python
<!-- Code example in Python -->
# Enable query timing
import logging
logging.basicConfig(level=logging.DEBUG)

# Or check database slow log
# PostgreSQL: SELECT * FROM pg_stat_statements ORDER BY mean_time DESC;
```text
<!-- Code example in TEXT -->

**Solutions**:

```python
<!-- Code example in Python -->
# Increase timeout in configuration
fraiseql_config = {
    'TIMEOUT': 60,  # seconds
}

# Optimize the SQL view/function
# Add indexes on filter columns
# Limit result set with pagination
@FraiseQL.query(sql_source="v_users")
def users(limit: int = 20, offset: int = 0) -> list[User]:
    """Paginate results to improve performance."""
    pass
```text
<!-- Code example in TEXT -->

#### Authentication Errors

**Issue**: `FraiseQLError: Authentication failed: token invalid`

**Debug**:

```python
<!-- Code example in Python -->
# Check if context has required auth info
@FraiseQL.query(sql_source="v_users")
@FraiseQL.security(requires_auth=True)
def my_users(context: dict) -> list[User]:
    """Verify context contains user info."""
    print(f"User ID: {context.get('user_id')}")
    pass
```text
<!-- Code example in TEXT -->

**Solutions**:

```python
<!-- Code example in Python -->
# Ensure auth context is passed
result = fraiseql_server.execute(
    query=query,
    context={"user_id": request.user.id}  # Must include
)

# Check token format (JWT, OAuth, etc.)
# Validate token signature
# Verify token hasn't expired
```text
<!-- Code example in TEXT -->

---

### Performance Issues

#### Query Performance

**Issue**: `Query took 5 seconds to execute`

**Debug with EXPLAIN**:

```sql
<!-- Code example in SQL -->
-- Check query plan
EXPLAIN ANALYZE SELECT * FROM v_users LIMIT 10;
```text
<!-- Code example in TEXT -->

**Solutions**:

```python
<!-- Code example in Python -->
# Add query result caching
@FraiseQL.query(sql_source="v_users", cache_ttl=300)
def users(limit: int = 10) -> list[User]:
    """Results cached for 5 minutes."""
    pass

# Pagination reduces memory/processing
@FraiseQL.query(sql_source="v_users")
def users(limit: int = 20, offset: int = 0) -> list[User]:
    """Limit results to reduce load."""
    pass

# Add database indexes
# CREATE INDEX idx_users_email ON users(email);
```text
<!-- Code example in TEXT -->

#### Memory Leaks

**Issue**: Application memory usage grows over time

**Debug**:

```python
<!-- Code example in Python -->
# Profile memory usage
import tracemalloc
tracemalloc.start()
# ... run queries ...
current, peak = tracemalloc.get_traced_memory()
print(f"Current: {current / 1024 / 1024}MB; Peak: {peak / 1024 / 1024}MB")
```text
<!-- Code example in TEXT -->

**Common causes**:

- Unbounded result sets (missing `limit`)
- Connection pool not releasing
- Schema objects not cleaned up

**Solutions**:

```python
<!-- Code example in Python -->
# Always paginate
@FraiseQL.query(sql_source="v_data")
def large_dataset(limit: int = 100) -> list[Data]:
    """Default limit prevents memory explosion."""
    pass

# Close connections explicitly
server.close()  # or use context manager
with FraiseQLServer.from_compiled("schema.json") as server:
    result = server.execute(query)
```text
<!-- Code example in TEXT -->

#### Connection Pooling

**Issue**: `Too many open connections to database`

**Debug connection count**:

```sql
<!-- Code example in SQL -->
-- PostgreSQL
SELECT count(*) FROM pg_stat_activity WHERE datname = 'mydb';
```text
<!-- Code example in TEXT -->

**Solution - Configure pool size**:

```python
<!-- Code example in Python -->
server = FraiseQLServer.from_compiled(
    "schema.compiled.json",
    pool_size=20,           # Max connections
    pool_min_size=5,        # Min idle connections
    pool_recycle=3600       # Recycle connections after 1 hour
)
```text
<!-- Code example in TEXT -->

#### Caching Misses

**Issue**: No performance improvement despite enabling `cache_ttl`

**Verify cache**:

```python
<!-- Code example in Python -->
# Enable debug logging to see cache hits/misses
import logging
logging.getLogger('FraiseQL').setLevel(logging.DEBUG)

# Check cache statistics
stats = server.cache_stats()
print(f"Hits: {stats['hits']}, Misses: {stats['misses']}")
```text
<!-- Code example in TEXT -->

**Ensure cache is actually used**:

```python
<!-- Code example in Python -->
# Each different query/variable combo is cached separately
query1 = "query { users(limit: 10) { id } }"
query2 = "query { users(limit: 20) { id } }"  # Different query = cache miss

# Use same queries with different variables for cache hits
result1 = server.execute(query, variables={"limit": 10})
result2 = server.execute(query, variables={"limit": 10})  # Cache hit!
```text
<!-- Code example in TEXT -->

---

### Debugging Techniques

#### Enable Debug Logging

**Setup logging**:

```python
<!-- Code example in Python -->
import logging
logging.basicConfig(
    level=logging.DEBUG,
    format='%(name)s - %(levelname)s - %(message)s'
)

# Only FraiseQL logs
logging.getLogger('FraiseQL').setLevel(logging.DEBUG)

# SQL query logging
logging.getLogger('FraiseQL.sql').setLevel(logging.DEBUG)
```text
<!-- Code example in TEXT -->

**Environment variable**:

```bash
<!-- Code example in BASH -->
RUST_LOG=FraiseQL=debug python app.py
```text
<!-- Code example in TEXT -->

#### Use Language Debugger

**PDB (Python Debugger)**:

```python
<!-- Code example in Python -->
@FraiseQL.query(sql_source="v_users")
def users(limit: int = 10) -> list[User]:
    breakpoint()  # Pauses here
    pass
```text
<!-- Code example in TEXT -->

**Run with debugger**:

```bash
<!-- Code example in BASH -->
python -m pdb app.py
```text
<!-- Code example in TEXT -->

#### Inspect Generated Schemas

**Print compiled schema**:

```python
<!-- Code example in Python -->
import json
with open("schema.compiled.json") as f:
    compiled = json.load(f)
    print(json.dumps(compiled, indent=2))
```text
<!-- Code example in TEXT -->

**Check generated GraphQL**:

```python
<!-- Code example in Python -->
# Introspection query
result = server.execute("""
    query {
        __schema {
            types {
                name
                kind
            }
        }
    }
""")
print(json.dumps(result, indent=2))
```text
<!-- Code example in TEXT -->

#### Monitor Network Traffic

**Using tcpdump**:

```bash
<!-- Code example in BASH -->
tcpdump -i lo -A 'tcp port 5432'  # Monitor PostgreSQL
```text
<!-- Code example in TEXT -->

**Using curl**:

```bash
<!-- Code example in BASH -->
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ user(id: 1) { id } }"}' \
  -v  # Verbose output
```text
<!-- Code example in TEXT -->

---

### Getting Help

#### GitHub Issues

When reporting issues, provide:

1. Python version: `python --version`
2. FraiseQL version: `pip show FraiseQL`
3. Minimal reproducible example
4. Error traceback
5. Relevant logs

**Issue template**:

```markdown
<!-- Code example in MARKDOWN -->
**Environment**:
- Python: 3.12
- FraiseQL: 2.0.0
- Database: PostgreSQL 15

**Issue**:
[Describe problem]

**Reproduce**:
[Minimal code example]

**Error**:
[Full traceback]
```text
<!-- Code example in TEXT -->

#### Community Channels

- **GitHub Discussions**: Ask questions and get help from community
- **Stack Overflow**: Tag with `FraiseQL` and `python`
- **Discord**: Real-time chat with maintainers and community

#### Performance Profiling

**Use cProfile**:

```python
<!-- Code example in Python -->
import cProfile
import pstats

profiler = cProfile.Profile()
profiler.enable()

# Run queries
result = server.execute(query)

profiler.disable()
stats = pstats.Stats(profiler)
stats.sort_stats('cumulative')
stats.print_stats(10)  # Top 10 functions
```text
<!-- Code example in TEXT -->

#### Database Query Analysis

**Enable PostgreSQL query logging**:

```sql
<!-- Code example in SQL -->
ALTER DATABASE mydb SET log_statement = 'all';
ALTER DATABASE mydb SET log_duration = 'on';
```text
<!-- Code example in TEXT -->

**Analyze query plan**:

```python
<!-- Code example in Python -->
def explain_query(view_name):
    conn = psycopg2.connect(DATABASE_URL)
    cursor = conn.cursor()
    cursor.execute(f"EXPLAIN ANALYZE SELECT * FROM {view_name}")
    for row in cursor.fetchall():
        print(row)
```text
<!-- Code example in TEXT -->

---

**Status**: ✅ Production Ready
**Last Updated**: 2026-02-05
**Maintained By**: FraiseQL Community
**License**: MIT
