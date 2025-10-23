# Database API

Repository pattern for async database operations with type safety, structured queries, and JSONB views.

**📍 Navigation**: [← Queries & Mutations](queries-and-mutations.md) • [Performance →](../performance/index.md) • [Database Patterns →](../advanced/database-patterns.md)

## Overview

FraiseQL provides a repository layer for database operations that:
- Executes structured queries against JSONB views
- Supports dynamic filtering with operators
- Handles pagination and ordering
- Provides tenant isolation
- Returns RustResponseBytes for automatic GraphQL processing

## Query Flow Architecture

### Repository Query Execution

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│ GraphQL     │───▶│ Repository  │───▶│ PostgreSQL  │───▶│   Rust      │
│ Resolver    │    │  Method     │    │   View      │    │ Pipeline    │
│             │    │             │    │             │    │             │
│ @query      │    │ find_rust() │    │ SELECT *    │    │ Transform   │
│ def users:  │    │             │    │ FROM v_user │    │ JSONB→GraphQL│
└─────────────┘    └─────────────┘    └─────────────┘    └─────────────┘
```

**Query Flow Steps:**
1. **GraphQL Resolver** calls repository method with filters
2. **Repository** builds SQL query with WHERE clauses and pagination
3. **PostgreSQL** executes view and returns JSONB results
4. **Rust Pipeline** transforms JSONB to GraphQL response format

**[📊 Detailed Query Flow](diagrams/request-flow.md)** - Complete request lifecycle

## FraiseQLRepository

Core repository class for async database operations with exclusive Rust pipeline integration.

### Key Methods

#### find_rust(view_name, field_name, info, **kwargs)
Execute query using exclusive Rust pipeline and return RustResponseBytes.

**Fastest method** - PostgreSQL → Rust → HTTP with zero Python string operations.

```python
# Exclusive Rust pipeline methods:
users = await repo.find_rust("v_user", "users", info)
user = await repo.find_one_rust("v_user", "user", info, id=123)
filtered = await repo.find_rust("v_user", "users", info, age__gt=18)
```

**Parameters:**
- `view_name: str` - Database view name (e.g., "v_user")
- `field_name: str` - GraphQL field name for response wrapping
- `info: Any` - GraphQL resolver info for field paths
- `**kwargs` - Filter parameters and options

**Returns:** `RustResponseBytes` - Pre-serialized GraphQL response ready for HTTP

#### find_one_rust(view_name, field_name, info, **kwargs)
Execute single-result query using exclusive Rust pipeline.

**Parameters:**
- `view_name: str` - Database view name
- `field_name: str` - GraphQL field name for response wrapping
- `info: Any` - GraphQL resolver info for field paths
- `**kwargs` - Filter parameters

**Returns:** `RustResponseBytes` - Single result as GraphQL response

#### find(source, where=None, **kwargs)
Execute query and return Python objects.

```python
# Direct database access (bypasses Rust pipeline)
users = await repo.find("v_user")
user = await repo.find_one("v_user", id=123)
```

**Parameters:**
- `source: str` - View name (e.g., "v_user")
- `where: dict` - WHERE clause filters (optional)
- `**kwargs` - Additional filters

**Returns:** Python objects (slower path)

### Initialization

```python
from psycopg_pool import AsyncConnectionPool

pool = AsyncConnectionPool(
    conninfo="postgresql://localhost/mydb",
    min_size=5,
    max_size=20
)

repo = PsycopgRepository(
    pool=pool,
    tenant_id="tenant-123"  # Optional: tenant context
)
```

**Parameters**:
| Name | Type | Required | Description |
|------|------|----------|-------------|
| pool | AsyncConnectionPool | Yes | Connection pool instance |
| tenant_id | str | None | No | Tenant identifier for multi-tenant contexts |

### select_from_json_view()

Primary method for querying JSONB views with filtering, pagination, and ordering.

**Signature**:
```python
async def select_from_json_view(
    self,
    tenant_id: uuid.UUID,
    view_name: str,
    *,
    options: QueryOptions | None = None,
) -> tuple[Sequence[dict[str, object]], int | None]
```

**Parameters**:
| Name | Type | Required | Description |
|------|------|----------|-------------|
| tenant_id | UUID | Yes | Tenant identifier for multi-tenant filtering |
| view_name | str | Yes | Database view name (e.g., "v_orders") |
| options | QueryOptions | None | No | Query options (filters, pagination, ordering) |

**Returns**: `tuple[Sequence[dict[str, object]], int | None]`
- First element: List of result dictionaries from json_data column
- Second element: Total count (if paginated), None otherwise

**Example**:
```python
from fraiseql.db import PsycopgRepository, QueryOptions
from fraiseql.db.pagination import (
    PaginationInput,
    OrderByInstructions,
    OrderByInstruction,
    OrderDirection
)

repo = PsycopgRepository(connection_pool)

options = QueryOptions(
    filters={
        "status": "active",
        "created_at__min": "2024-01-01",
        "price__max": 100.00
    },
    order_by=OrderByInstructions(
        instructions=[
            OrderByInstruction(field="created_at", direction=OrderDirection.DESC)
        ]
    ),
    pagination=PaginationInput(limit=50, offset=0)
)

data, total = await repo.select_from_json_view(
    tenant_id=tenant_id,
    view_name="v_orders",
    options=options
)

print(f"Retrieved {len(data)} orders out of {total} total")
for order in data:
    print(f"Order {order['id']}: {order['status']}")
```

### fetch_one()

Fetch single row from database.

**Signature**:
```python
async def fetch_one(
    self,
    query: Composed,
    args: tuple[object, ...] = ()
) -> dict[str, object]
```

**Parameters**:
| Name | Type | Required | Description |
|------|------|----------|-------------|
| query | Composed | Yes | Psycopg Composed SQL query |
| args | tuple | () | No | Query parameters |

**Returns**: Dictionary representing single row

**Raises**:
- `ValueError` - No row returned
- `DatabaseConnectionError` - Connection failure
- `DatabaseQueryError` - Query execution error

**Example**:
```python
from psycopg.sql import SQL, Identifier, Placeholder

query = SQL("SELECT json_data FROM {} WHERE id = {}").format(
    Identifier("v_user"),
    Placeholder()
)

user = await repo.fetch_one(query, (user_id,))
```

### fetch_all()

Fetch all rows from database query.

**Signature**:
```python
async def fetch_all(
    self,
    query: Composed,
    args: tuple[object, ...] = ()
) -> list[dict[str, object]]
```

**Parameters**:
| Name | Type | Required | Description |
|------|------|----------|-------------|
| query | Composed | Yes | Psycopg Composed SQL query |
| args | tuple | () | No | Query parameters |

**Returns**: List of dictionaries representing all rows

**Example**:
```python
query = SQL("SELECT json_data FROM {} WHERE tenant_id = {}").format(
    Identifier("v_orders"),
    Placeholder()
)

orders = await repo.fetch_all(query, (tenant_id,))
```

### execute()

Execute query without returning results (INSERT, UPDATE, DELETE).

**Signature**:
```python
async def execute(
    self,
    query: Composed,
    args: tuple[object, ...] = ()
) -> None
```

**Example**:
```python
query = SQL("UPDATE {} SET status = {} WHERE id = {}").format(
    Identifier("tb_orders"),
    Placeholder(),
    Placeholder()
)

await repo.execute(query, ("shipped", order_id))
```

### execute_many()

Execute query multiple times with different parameters in single transaction.

**Signature**:
```python
async def execute_many(
    self,
    query: Composed,
    args_list: list[tuple[object, ...]]
) -> None
```

**Example**:
```python
query = SQL("INSERT INTO {} (name, email) VALUES ({}, {})").format(
    Identifier("tb_users"),
    Placeholder(),
    Placeholder()
)

await repo.execute_many(query, [
    ("Alice", "alice@example.com"),
    ("Bob", "bob@example.com"),
    ("Charlie", "charlie@example.com")
])
```

## QueryOptions

Structured query parameters for filtering, pagination, and ordering.

**Definition**:
```python
@dataclass
class QueryOptions:
    aggregations: dict[str, str] | None = None
    order_by: OrderByInstructions | None = None
    dimension_key: str | None = None
    pagination: PaginationInput | None = None
    filters: dict[str, object] | None = None
    where: ToSQLProtocol | None = None
    ignore_tenant_column: bool = False
```

**Fields**:
| Field | Type | Default | Description |
|-------|------|---------|-------------|
| aggregations | dict[str, str] | None | None | Aggregation functions (SUM, AVG, COUNT, MIN, MAX) |
| order_by | OrderByInstructions | None | None | Ordering specifications |
| dimension_key | str | None | None | JSON dimension key for nested ordering |
| pagination | PaginationInput | None | None | Pagination parameters (limit, offset) |
| filters | dict[str, object] | None | None | Dynamic filters with operators |
| where | ToSQLProtocol | None | None | Custom WHERE clause object |
| ignore_tenant_column | bool | False | False | Bypass tenant filtering |

## Dynamic Filters

Filter syntax supports multiple operators for flexible querying.

### Supported Operators

| Operator | SQL Equivalent | Example | Description |
|----------|----------------|---------|-------------|
| (none) | = | `{"status": "active"}` | Exact match |
| __min | >= | `{"created_at__min": "2024-01-01"}` | Greater than or equal |
| __max | <= | `{"price__max": 100}` | Less than or equal |
| __in | IN | `{"status__in": ["active", "pending"]}` | Match any value in list |
| __contains | <@ | `{"path__contains": "electronics"}` | ltree path containment |

**NULL Handling**:
```python
filters = {
    "description": None  # Translates to: WHERE description IS NULL
}
```

### Filter Examples

**Simple equality**:
```python
options = QueryOptions(
    filters={"status": "active"}
)
# SQL: WHERE status = 'active'
```

**Range queries**:
```python
options = QueryOptions(
    filters={
        "created_at__min": "2024-01-01",
        "created_at__max": "2024-12-31",
        "price__min": 10.00,
        "price__max": 100.00
    }
)
# SQL: WHERE created_at >= '2024-01-01' AND created_at <= '2024-12-31'
#      AND price >= 10.00 AND price <= 100.00
```

**IN operator**:
```python
options = QueryOptions(
    filters={
        "status__in": ["active", "pending", "processing"]
    }
)
# SQL: WHERE status IN ('active', 'pending', 'processing')
```

**Multiple conditions**:
```python
options = QueryOptions(
    filters={
        "category": "electronics",
        "price__max": 500.00,
        "in_stock": True,
        "vendor__in": ["vendor-a", "vendor-b"]
    }
)
# SQL: WHERE category = 'electronics'
#      AND price <= 500.00
#      AND in_stock = TRUE
#      AND vendor IN ('vendor-a', 'vendor-b')
```

## Nested Object Filtering

FraiseQL v1.0.0+ supports filtering on nested objects stored in JSONB columns.

### Basic Nested Filter

Filter on nested JSONB objects using dot notation:

```python
# Dictionary-based filtering
where = {
    "machine": {
        "name": {"eq": "Server-01"}
    }
}
results = await repo.find("allocations", where=where)
# SQL: WHERE data->'machine'->>'name' = 'Server-01'
```

### Multiple Nesting Levels

```python
where = {
    "location": {
        "address": {
            "city": {"eq": "Seattle"}
        }
    }
}
# SQL: WHERE data->'location'->'address'->>'city' = 'Seattle'
```

### Combined Filters

Mix flat and nested filters:

```python
where = {
    "status": {"eq": "active"},
    "machine": {
        "type": {"eq": "Server"},
        "power": {"gte": 100}
    }
}
# SQL: WHERE data->>'status' = 'active'
#      AND data->'machine'->>'type' = 'Server'
#      AND data->'machine'->>'power' >= 100
```

### GraphQL WhereInput Objects

Use generated WhereInput types for type-safe filtering:

```python
from fraiseql.sql import create_graphql_where_input

MachineWhereInput = create_graphql_where_input(Machine)
AllocationWhereInput = create_graphql_where_input(Allocation)

where = AllocationWhereInput(
    machine=MachineWhereInput(
        name=StringFilter(eq="Server-01")
    )
)
results = await repo.find("allocations", where=where)
```

### Supported Operators

All standard operators work with nested objects:
- `eq`, `neq` - equality/inequality
- `gt`, `gte`, `lt`, `lte` - comparisons
- `in`, `notin` - list membership
- `contains`, `startswith`, `endswith` - string patterns
- `is_null` - null checks

## Coordinate Filtering

FraiseQL v1.0.0+ supports geographic coordinate filtering with PostgreSQL POINT type casting.

### Basic Coordinate Equality

Filter by exact coordinate match:

```python
# Dictionary-based filtering
where = {
    "coordinates": {"eq": (45.5, -122.6)}  # (latitude, longitude)
}
results = await repo.find("locations", where=where)
# SQL: WHERE (data->>'coordinates')::point = POINT(-122.6, 45.5)
```

### Coordinate List Operations

Check if coordinates are in a list:

```python
where = {
    "coordinates": {"in": [
        (45.5, -122.6),  # Seattle
        (47.6097, -122.3425),  # Pike Place
        (40.7128, -74.0060)  # NYC
    ]}
}
# SQL: WHERE (data->>'coordinates')::point IN (POINT(-122.6, 45.5), ...)
```

### Distance-Based Filtering

Find locations within distance:

```python
where = {
    "coordinates": {
        "distance_within": ((45.5, -122.6), 5000)  # Center point, radius in meters
    }
}
```

FraiseQL supports three distance calculation methods:

1. **Haversine Formula** (default, no dependencies)
   - Pure SQL implementation using great-circle distance
   - Accuracy: ±0.5% for distances < 1000km
   - Works with standard PostgreSQL

2. **PostGIS ST_DWithin** (most accurate)
   - Geodesic distance on spheroid model
   - Accuracy: ±0.1% at any distance
   - Requires: `CREATE EXTENSION postgis;`

3. **earthdistance** (moderate accuracy)
   - PostgreSQL earthdistance extension
   - Accuracy: ±1-2%
   - Requires: `CREATE EXTENSION earthdistance;`

#### Configuration

Set the distance method in your config:

```python
from fraiseql.fastapi import FraiseQLConfig

config = FraiseQLConfig(
    database_url="postgresql://...",
    coordinate_distance_method="haversine"  # default
    # or "postgis" for production
    # or "earthdistance" for legacy systems
)
```

Or via environment variable:

```bash
export FRAISEQL_COORDINATE_DISTANCE_METHOD=postgis
```

### Coordinate Operators

- `eq`, `neq` - exact coordinate equality
- `in`, `notin` - coordinate list membership
- `distance_within` - distance-based filtering

**Note**: Coordinates are stored as `(latitude, longitude)` tuples but converted to PostgreSQL `POINT(longitude, latitude)` for spatial operations.

## Pagination

Efficient pagination using ROW_NUMBER() window function.

### PaginationInput

**Definition**:
```python
@dataclass
class PaginationInput:
    limit: int | None = None
    offset: int | None = None
```

**Fields**:
| Field | Type | Default | Description |
|-------|------|---------|-------------|
| limit | int | None | None | Maximum number of results (default: 250) |
| offset | int | None | None | Number of results to skip (default: 0) |

**Example**:
```python
# Page 1
options = QueryOptions(
    pagination=PaginationInput(limit=20, offset=0)
)

# Page 2
options = QueryOptions(
    pagination=PaginationInput(limit=20, offset=20)
)

# Page 3
options = QueryOptions(
    pagination=PaginationInput(limit=20, offset=40)
)
```

### Pagination SQL Pattern

FraiseQL uses efficient ROW_NUMBER() pagination:

```sql
WITH paginated_cte AS (
    SELECT json_data,
           ROW_NUMBER() OVER (ORDER BY created_at DESC) AS row_num
    FROM v_orders
    WHERE tenant_id = $1
)
SELECT * FROM paginated_cte
WHERE row_num BETWEEN $2 AND $3
```

**Benefits**:
- Consistent results across pages
- Works with complex ORDER BY clauses
- Efficient for moderate offsets
- Returns total count separately

## Ordering

Structured ordering with support for native columns, JSON fields, and aggregations.

### OrderByInstructions

**Definition**:
```python
@dataclass
class OrderByInstructions:
    instructions: list[OrderByInstruction]

@dataclass
class OrderByInstruction:
    field: str
    direction: OrderDirection

class OrderDirection(Enum):
    ASC = "asc"
    DESC = "desc"
```

**Example**:
```python
options = QueryOptions(
    order_by=OrderByInstructions(
        instructions=[
            OrderByInstruction(field="created_at", direction=OrderDirection.DESC),
            OrderByInstruction(field="total_amount", direction=OrderDirection.ASC)
        ]
    )
)
```

### Ordering Patterns

**Native column ordering**:
```python
order_by=OrderByInstructions(instructions=[
    OrderByInstruction(field="created_at", direction=OrderDirection.DESC)
])
# SQL: ORDER BY created_at DESC
```

**JSON field ordering**:
```python
order_by=OrderByInstructions(instructions=[
    OrderByInstruction(field="customer_name", direction=OrderDirection.ASC)
])
# SQL: ORDER BY json_data->>'customer_name' ASC
```

**Aggregation ordering**:
```python
options = QueryOptions(
    aggregations={"total": "SUM"},
    order_by=OrderByInstructions(instructions=[
        OrderByInstruction(field="total", direction=OrderDirection.DESC)
    ])
)
# SQL: SUM(total) AS total_agg ORDER BY total_agg DESC
```

## Multi-Tenancy

Automatic tenant filtering for multi-tenant applications.

### Tenant Column Detection

```python
from fraiseql.db.utils import get_tenant_column

tenant_info = get_tenant_column(view_name="v_orders")
# Returns: {"table": "tenant_id", "view": "tenant_id"}
```

**Tenant column mapping**:
- **Tables**: `tenant_id` - Foreign key to tenant table
- **Views**: `tenant_id` - Denormalized tenant identifier

### Automatic Filtering

Repository automatically adds tenant filter to all queries:

```python
repo = PsycopgRepository(pool, tenant_id="tenant-123")

# This query:
data, total = await repo.select_from_json_view(
    tenant_id=tenant_id,
    view_name="v_orders"
)

# Automatically adds: WHERE tenant_id = $1
```

### Bypassing Tenant Filtering

For admin queries that need cross-tenant access:

```python
options = QueryOptions(
    ignore_tenant_column=True
)

data, total = await repo.select_from_json_view(
    tenant_id=tenant_id,
    view_name="v_orders",
    options=options
)
# No tenant_id filter applied
```

## SQL Builder Utilities

Low-level utilities for constructing dynamic SQL queries.

### build_filter_conditions_and_params()

**Signature**:
```python
def build_filter_conditions_and_params(
    filters: dict[str, object]
) -> tuple[list[str], tuple[Scalar | ScalarList, ...]]
```

**Returns**: Tuple of (condition strings, parameters)

**Example**:
```python
from fraiseql.db.sql_builder import (
    build_filter_conditions_and_params
)

filters = {
    "status": "active",
    "price__min": 10.00,
    "tags__in": ["electronics", "gadgets"]
}

conditions, params = build_filter_conditions_and_params(filters)
# conditions: ["status = %s", "price >= %s", "tags IN (%s, %s)"]
# params: ("active", 10.00, "electronics", "gadgets")
```

### generate_order_by_clause()

**Signature**:
```python
def generate_order_by_clause(
    order_by: OrderByInstructions,
    aggregations: dict[str, str],
    view_name: str,
    alias_mapping: dict[str, str] | None = None,
    dimension_key: str | None = None
) -> tuple[Composed, list[Composed]]
```

**Returns**: Tuple of (ORDER BY clause, aggregated column expressions)

### generate_pagination_query()

**Signature**:
```python
def generate_pagination_query(
    base_query: Composable,
    order_by_clause: Composable,
    aggregated_columns: Sequence[Composed],
    pagination: PaginationInput | None
) -> tuple[Composed, tuple[int, int]]
```

**Returns**: Tuple of (paginated query, (start_row, end_row))

## Error Handling

Custom exceptions for database operations.

### Exception Hierarchy

```python
from fraiseql.db.exceptions import (
    DatabaseConnectionError,    # Connection pool or network errors
    DatabaseQueryError,          # SQL execution errors
    InvalidFilterError           # Filter validation errors
)
```

**Usage**:
```python
try:
    data, total = await repo.select_from_json_view(
        tenant_id=tenant_id,
        view_name="v_orders",
        options=options
    )
except DatabaseConnectionError as e:
    logger.error(f"Database connection failed: {e}")
    # Retry logic or fallback
except DatabaseQueryError as e:
    logger.error(f"Query execution failed: {e}")
    # Check query syntax
except InvalidFilterError as e:
    logger.error(f"Invalid filter provided: {e}")
    # Validate filter input
```

## Type Safety

Repository uses Protocol-based typing for extensibility.

### ToSQLProtocol

Interface for objects that can generate SQL clauses:

```python
class ToSQLProtocol(Protocol):
    def to_sql(self, view_name: str) -> Composed:
        ...
```

**Example implementation**:
```python
from psycopg.sql import SQL, Identifier, Placeholder

class CustomFilter:
    def __init__(self, field: str, value: object):
        self.field = field
        self.value = value

    def to_sql(self, view_name: str) -> Composed:
        return SQL("{} = {}").format(
            Identifier(self.field),
            Placeholder()
        )

custom_filter = CustomFilter("status", "active")
options = QueryOptions(where=custom_filter)
```

## Best Practices

**Use structured queries**:
```python
# Good: Structured with QueryOptions
options = QueryOptions(
    filters={"status": "active"},
    pagination=PaginationInput(limit=50, offset=0),
    order_by=OrderByInstructions(instructions=[...])
)
data, total = await repo.select_from_json_view(tenant_id, "v_orders", options=options)

# Avoid: Raw SQL strings
query = "SELECT * FROM v_orders WHERE status = 'active' LIMIT 50"
```

**Use connection pooling**:
```python
# Good: Shared connection pool
pool = AsyncConnectionPool(conninfo=DATABASE_URL, min_size=5, max_size=20)
repo = PsycopgRepository(pool)

# Avoid: Creating connections per request
```

**Handle pagination correctly**:
```python
# Good: Check total count
data, total = await repo.select_from_json_view(
    tenant_id, "v_orders",
    options=QueryOptions(pagination=PaginationInput(limit=20, offset=0))
)
has_next_page = len(data) + offset < total

# Avoid: Assuming more results exist
```

**Use tenant filtering**:
```python
# Good: Automatic tenant isolation
data, total = await repo.select_from_json_view(tenant_id, "v_orders")

# Avoid: Manual tenant filtering in WHERE clauses
```

## Complete Example

```python
import uuid
from psycopg_pool import AsyncConnectionPool
from fraiseql.db import PsycopgRepository, QueryOptions
from fraiseql.db.pagination import (
    PaginationInput,
    OrderByInstructions,
    OrderByInstruction,
    OrderDirection
)

# Initialize repository
pool = AsyncConnectionPool(
    conninfo="postgresql://localhost/mydb",
    min_size=5,
    max_size=20
)
repo = PsycopgRepository(pool)

# Query with filtering, pagination, and ordering
tenant_id = uuid.uuid4()
options = QueryOptions(
    filters={
        "status__in": ["active", "pending"],
        "created_at__min": "2024-01-01",
        "total_amount__min": 100.00
    },
    order_by=OrderByInstructions(
        instructions=[
            OrderByInstruction(field="created_at", direction=OrderDirection.DESC)
        ]
    ),
    pagination=PaginationInput(limit=20, offset=0)
)

data, total = await repo.select_from_json_view(
    tenant_id=tenant_id,
    view_name="v_orders",
    options=options
)

print(f"Retrieved {len(data)} of {total} orders")
for order in data:
    print(f"Order {order['id']}: ${order['total_amount']}")
```

## See Also

- [Queries & Mutations](queries-and-mutations.md) - Using repository methods in GraphQL resolvers
- [Database Patterns](../advanced/database-patterns.md) - View design and N+1 prevention
- [Performance](../performance/index.md) - Query optimization
- [Multi-Tenancy](../advanced/multi-tenancy.md) - Tenant isolation patterns
