# Decorators Reference

Complete API reference for FraiseQL decorators.

## Table of Contents

1. [@type](#type) - Define GraphQL types
2. [@query](#query) - Define read queries
3. [@mutation](#mutation) - Define write operations
4. [@fact_table](#fact_table) - Define analytics tables
5. [@aggregate_query](#aggregate_query) - Define OLAP queries
6. [config()](#config) - Configuration helper

---

## @type

Mark a Python class as a GraphQL type.

### Signature

```python
@fraiseql.type
class MyType:
    field1: int
    field2: str
```

### Features

- **Type annotations required**: All fields must be type-annotated
- **Nullable support**: Use `| None` for nullable fields
- **Nested types**: Reference other `@fraiseql.type` classes
- **Lists**: Use `list[T]` for array types
- **Documentation**: Docstring becomes type description

### Examples

```python
# Simple type
@fraiseql.type
class User:
    id: int
    name: str
    email: str

# With nullable fields
@fraiseql.type
class Post:
    id: int
    title: str
    body: str
    published_at: str | None

# With lists
@fraiseql.type
class Blog:
    id: int
    title: str
    tags: list[str]

# Nested types
@fraiseql.type
class Address:
    street: str
    city: str

@fraiseql.type
class Person:
    id: int
    name: str
    address: Address
```

### Type Mapping

| Python | GraphQL | Notes |
|--------|---------|-------|
| `int` | `Int` | |
| `float` | `Float` | |
| `str` | `String` | |
| `bool` | `Boolean` | |
| `list[T]` | `[T!]` | Lists contain non-null elements |
| `T \| None` | `T` | Nullable type |
| `list[T \| None]` | `[T]` | List of nullable elements |
| Custom class | Object | Must be decorated with @fraiseql.type |

---

## @query

Define a read-only GraphQL query.

### Signature

```python
@fraiseql.query(sql_source="view_name")
def my_query(arg1: int, arg2: str = "default") -> ResultType:
    """Query description."""
    pass
```

### Parameters

- `sql_source` (optional): SQL view name to query
- `auto_params` (optional): Dict of parameter configurations

### Features

- **List queries**: Return `list[Type]` for multiple results
- **Single result**: Return `Type | None` for optional single result
- **Default arguments**: Use Python defaults
- **Type-safe**: Python types → GraphQL types
- **Documentation**: Docstring becomes query description

### Examples

```python
# Simple list query
@fraiseql.query(sql_source="v_users")
def users(limit: int = 10) -> list[User]:
    """Get all users."""
    pass

# Query by ID
@fraiseql.query(sql_source="v_user_by_id")
def user(id: int) -> User | None:
    """Get a user by ID."""
    pass

# Multiple parameters
@fraiseql.query(sql_source="v_search_users")
def search_users(
    name: str,
    email: str | None = None,
    limit: int = 20,
    offset: int = 0
) -> list[User]:
    """Search users by name and email."""
    pass

# Query without SQL source (for functions)
@fraiseql.query
def health_check() -> bool:
    """Check server health."""
    pass
```

### Argument Handling

All arguments become GraphQL query arguments:

```python
@fraiseql.query(sql_source="v_users")
def users(
    limit: int = 10,        # Optional with default
    offset: int = 0,        # Optional with default
    active: bool = True,    # Optional with default
    search: str | None = None  # Optional, explicitly nullable
) -> list[User]:
    pass
```

Generates GraphQL:

```graphql
type Query {
  users(
    limit: Int = 10
    offset: Int = 0
    active: Boolean = true
    search: String  # nullable
  ): [User!]!
}
```

---

## @mutation

Define a write operation (create, update, delete).

### Signature

```python
@fraiseql.mutation(
    sql_source="function_name",
    operation="CREATE"
)
def my_mutation(arg1: str) -> ResultType:
    """Mutation description."""
    pass
```

### Parameters

- `sql_source` (required): SQL function name
- `operation` (optional): Type of operation
  - `"CREATE"` - Insert new data
  - `"UPDATE"` - Modify existing data
  - `"DELETE"` - Remove data
  - `"CUSTOM"` - Custom operation

### Features

- **Type-safe**: Returns GraphQL type
- **Parameters**: All arguments become mutation inputs
- **Side effects**: Modifies database
- **Transactions**: Wrapped in SQL transactions
- **Documentation**: Docstring becomes description

### Examples

```python
# Create mutation
@fraiseql.mutation(
    sql_source="fn_create_user",
    operation="CREATE"
)
def create_user(name: str, email: str) -> User:
    """Create a new user."""
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
    """Update a user."""
    pass

# Delete mutation
@fraiseql.mutation(
    sql_source="fn_delete_user",
    operation="DELETE"
)
def delete_user(id: int) -> bool:
    """Delete a user and return success."""
    pass

# Batch operation
@fraiseql.mutation(
    sql_source="fn_bulk_update_users",
    operation="UPDATE"
)
def bulk_update_users(ids: list[int], status: str) -> list[User]:
    """Update multiple users at once."""
    pass
```

---

## @fact_table

Define an analytics fact table for OLAP queries.

### Signature

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

### Parameters

- `table_name` (required): SQL table name (must start with `tf_`)
- `measures` (required): List of measure column names (numeric)
- `dimension_column` (optional): JSONB column name (default: "data")
- `dimension_paths` (optional): List of dimension path definitions

### Dimension Path Structure

```python
{
    "name": "category",              # Path name (used in GROUP BY)
    "json_path": "data->>'category'",  # PostgreSQL JSON path
    "data_type": "text"              # SQL type hint
}
```

### Examples

```python
@fraiseql.fact_table(
    table_name="tf_sales",
    measures=["revenue", "quantity", "cost"],
    dimension_paths=[
        {
            "name": "category",
            "json_path": "data->>'category'",
            "data_type": "text"
        },
        {
            "name": "region",
            "json_path": "data->>'region'",
            "data_type": "text"
        },
        {
            "name": "product",
            "json_path": "data->>'product'",
            "data_type": "text"
        }
    ]
)
@fraiseql.type
class Sale:
    id: int
    revenue: float        # Measure
    quantity: int        # Measure
    cost: float         # Measure
    customer_id: str    # Denormalized filter
    occurred_at: str    # Denormalized filter
```

### SQL Table Pattern

```sql
CREATE TABLE tf_sales (
    id BIGSERIAL PRIMARY KEY,
    -- Measures (for aggregation)
    revenue DECIMAL(10,2) NOT NULL,
    quantity INT NOT NULL,
    cost DECIMAL(10,2) NOT NULL,
    -- Dimensions (for GROUP BY)
    data JSONB NOT NULL,
    -- Denormalized filters (indexed)
    customer_id UUID NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX ON tf_sales(customer_id);
CREATE INDEX ON tf_sales(occurred_at);
```

---

## @aggregate_query

Define an OLAP query on a fact table.

### Signature

```python
@fraiseql.aggregate_query(
    fact_table="tf_sales",
    auto_group_by=True,
    auto_aggregates=True
)
@fraiseql.query
def sales_aggregate() -> list[dict]:
    """Flexible sales aggregation."""
    pass
```

### Parameters

- `fact_table` (required): Fact table name (from `@fraiseql.fact_table`)
- `auto_group_by` (optional): Auto-generate GROUP BY fields (default: True)
- `auto_aggregates` (optional): Auto-generate aggregate fields (default: True)

### Features

- **Auto GROUP BY**: Generates groupBy fields for each dimension
- **Auto Aggregates**: Generates aggregate functions (sum, avg, count, etc.)
- **Temporal Buckets**: Date/time dimensions get day/month/year buckets
- **Filtering**: Pre-aggregation (WHERE) and post-aggregation (HAVING)
- **Flexible**: Supports ad-hoc grouping and aggregation

### Examples

```python
# Simple aggregate query
@fraiseql.aggregate_query(
    fact_table="tf_sales",
    auto_group_by=True,
    auto_aggregates=True
)
@fraiseql.query
def sales_aggregate() -> list[dict]:
    """Sales by category and month."""
    pass

# Query without auto-generation
@fraiseql.aggregate_query(
    fact_table="tf_sales",
    auto_group_by=False,
    auto_aggregates=False
)
@fraiseql.query
def custom_sales() -> list[dict]:
    """Custom sales aggregation."""
    pass
```

### Generated GraphQL

Automatically generates:

```graphql
type SalesAggregate {
  # Dimensions
  category: String
  region: String
  product: String
  occurred_at_day: Date
  occurred_at_month: Date
  occurred_at_year: Date

  # Aggregates
  revenue_sum: Float
  revenue_avg: Float
  revenue_min: Float
  revenue_max: Float
  quantity_sum: Int
  cost_sum: Float
  count: Int
}

type Query {
  salesAggregate(
    groupBy: [String!]
    aggregates: [String!]
    where: {customer_id: String, occurred_at_range: DateRange}
    having: {revenue_sum_gt: Float}
    orderBy: [String!]
    limit: Int
    offset: Int
  ): [SalesAggregate!]!
}
```

---

## config()

Configuration helper for decorators.

### Signature

```python
@fraiseql.query
def my_query() -> Result:
    return fraiseql.config(
        sql_source="v_data",
        auto_params={"limit": True}
    )
```

### Features

- **In-function configuration**: Alternative to decorator arguments
- **Dynamic configuration**: Can be conditional (though not recommended)
- **Type-safe**: Returns configuration dict

### Examples

```python
# Basic usage
@fraiseql.query
def users() -> list[User]:
    return fraiseql.config(sql_source="v_user")

# With parameters
@fraiseql.query
def users(limit: int = 10) -> list[User]:
    return fraiseql.config(
        sql_source="v_user",
        auto_params={"limit": True, "offset": True}
    )

# Alternative to decorator arguments
# These are equivalent:

# Style 1: Decorator arguments
@fraiseql.query(sql_source="v_user")
def users1() -> list[User]:
    pass

# Style 2: config() in function body
@fraiseql.query
def users2() -> list[User]:
    return fraiseql.config(sql_source="v_user")
```

---

## Complete Example

```python
import fraiseql

# Step 1: Define types
@fraiseql.type
class User:
    """A user account."""
    id: int
    name: str
    email: str
    created_at: str

@fraiseql.type
class Sale:
    """A sales transaction."""
    id: int
    user_id: int
    amount: float
    created_at: str

# Step 2: Define queries
@fraiseql.query(sql_source="v_user")
def users(limit: int = 10) -> list[User]:
    """Get all users."""
    pass

@fraiseql.query(sql_source="v_user_by_id")
def user(id: int) -> User | None:
    """Get a user by ID."""
    pass

# Step 3: Define mutations
@fraiseql.mutation(sql_source="fn_create_user", operation="CREATE")
def create_user(name: str, email: str) -> User:
    """Create a new user."""
    pass

@fraiseql.mutation(sql_source="fn_update_user", operation="UPDATE")
def update_user(id: int, name: str | None = None) -> User:
    """Update a user."""
    pass

# Step 4: Define fact table
@fraiseql.fact_table(
    table_name="tf_sales",
    measures=["amount"],
    dimension_paths=[
        {"name": "product", "json_path": "data->>'product'", "data_type": "text"}
    ]
)
@fraiseql.type
class SaleFact:
    id: int
    amount: float
    user_id: int
    created_at: str

# Step 5: Define aggregate query
@fraiseql.aggregate_query(
    fact_table="tf_sales",
    auto_group_by=True,
    auto_aggregates=True
)
@fraiseql.query
def sales_by_product() -> list[dict]:
    """Sales aggregated by product."""
    pass

# Step 6: Export
if __name__ == "__main__":
    fraiseql.export_schema("schema.json")
```

---

## Best Practices

1. **Always use type annotations** - FraiseQL relies on Python types
2. **Use descriptive docstrings** - They become GraphQL descriptions
3. **Keep types simple** - Avoid over-nesting
4. **Use nullable sparingly** - Prefer explicit `| None` over implicit nullability
5. **Map to SQL views/functions** - Provide `sql_source` for all queries/mutations
6. **Use fact tables for analytics** - Not suitable for operational queries
7. **Document dimensions and measures** - Help future developers understand patterns

---

## Limitations

- ❌ No custom resolvers
- ❌ No directives
- ❌ No union types (only concrete types)
- ❌ No interfaces
- ❌ No input types (only scalars and lists)
- ❌ No circular references

These limitations are intentional to keep FraiseQL simple and fast.
