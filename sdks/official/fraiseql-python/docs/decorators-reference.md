# Decorators Reference

Complete API reference for FraiseQL decorators.

## Table of Contents

1. [@type](#type) - Define GraphQL types
2. [@query](#query) - Define read queries
3. [@mutation](#mutation) - Define write operations
4. [fact_table()](#fact_table) - Declare analytics fact tables
5. [config()](#config) - Configuration helper

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

## fact_table()

Declare an analytics fact table. `fact_table()` is a plain function, not a decorator: call
it at module level, next to your types.

A fact table `tf_<name>` gets two GraphQL roots, `<name>_aggregate` and `<name>_window`,
derived when the schema compiles. There is no aggregate query to declare.

### Signature

```python
fraiseql.fact_table(
    "tf_sales",
    measures=[{"name": "revenue", "sql_type": "numeric", "nullable": False}],
    dimensions={
        "name": "data",
        "paths": [
            {"name": "category", "json_path": "data->>'category'", "data_type": "text"}
        ],
    },
    denormalized_filters=[{"name": "occurred_at", "sql_type": "date", "indexed": True}],
    type_name="Sale",
)
```

### Parameters

- `table_name` (required, positional): SQL relation name, `tf_<name>`
- `measures` (required): List of measure columns, each `{"name", "sql_type", "nullable"}`,
  with an optional `"additivity"`
- `dimensions` (optional): The JSONB dimensions column, `{"name", "paths": [...]}`
  (default: `{"name": "data", "paths": []}`)
- `denormalized_filters` (optional): Flat columns to filter and bucket by, each
  `{"name", "sql_type", "indexed"}`, with an optional `"hierarchy"` naming the hierarchy an
  `ltree` path column belongs to
- `type_name` (optional): The type the fact table is read as. Every measure, denormalized
  filter and dimension path must be a field of it, and its read gates apply
- `row_scope` (optional): `"relation"` when the relation filters its own rows by tenant on a
  value the server sets per request; the server then adds no tenant predicate
- `native_measures` (optional): Measure paths stored as flat columns, `{path: column}`
- `native_dimension_mapping` (optional): Dimension paths stored as flat columns,
  `{path: column}`; each column must be a denormalized filter

Raises `ValueError` on a duplicate `table_name`, a `row_scope` other than `"relation"`, or an
unknown additivity kind.

### Dimension Path Structure

```python
{
    "name": "category",                # Path name (used in groupBy and where)
    "json_path": "data->>'category'",  # PostgreSQL JSON path into the dimensions column
    "data_type": "text"                # SQL type hint
}
```

### Measure Additivity

A measure that must not be summed over time declares its additivity. Kinds: `additive`
(the default), `semi_additive`, `delta`, `non_additive`.

```python
fraiseql.fact_table(
    "tf_account_day",
    measures=[
        {
            "name": "balance",
            "sql_type": "numeric",
            "nullable": False,
            "additivity": {
                "kind": "semi_additive",
                "over": "day",
                "using": "last",
                "entity": ["account_id"],
            },
        }
    ],
    denormalized_filters=[
        {"name": "account_id", "sql_type": "bigint", "indexed": True},
        {"name": "day", "sql_type": "date", "indexed": True},
    ],
)
```

### Examples

```python
@fraiseql.type
class Sale:
    revenue: float      # Measure
    quantity: int       # Measure
    cost: float         # Measure
    category: str       # Dimension path
    region: str         # Dimension path
    product: str        # Dimension path
    customer_id: str    # Denormalized filter
    occurred_at: str    # Denormalized filter

fraiseql.fact_table(
    "tf_sales",
    type_name="Sale",
    measures=[
        {"name": "revenue", "sql_type": "numeric", "nullable": False},
        {"name": "quantity", "sql_type": "int", "nullable": False},
        {"name": "cost", "sql_type": "numeric", "nullable": False},
    ],
    dimensions={
        "name": "data",
        "paths": [
            {"name": "category", "json_path": "data->>'category'", "data_type": "text"},
            {"name": "region", "json_path": "data->>'region'", "data_type": "text"},
            {"name": "product", "json_path": "data->>'product'", "data_type": "text"},
        ],
    },
    denormalized_filters=[
        {"name": "customer_id", "sql_type": "uuid", "indexed": True},
        {"name": "occurred_at", "sql_type": "timestamptz", "indexed": True},
    ],
)
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

### Derived GraphQL Roots

`tf_sales` is queried through `sales_aggregate` (grouping and aggregation) and
`sales_window` (window functions):

```graphql
{
  sales_aggregate(
    table: "tf_sales"
    groupBy: { category: true, occurred_at: "month" }
    aggregates: [{ revenue_sum: {} }, { count: {} }]
  )
}
```

A time bucket is spelled either `groupBy: { occurred_at: "month" }` or
`groupBy: { occurred_at_month: true }`. Aggregates are named `<measure>_<function>`
(`revenue_sum`, `revenue_avg`, `revenue_min`, `revenue_max`, ...) plus `count`.

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

# Step 4: Declare the fact table (roots sales_aggregate and sales_window)
fraiseql.fact_table(
    "tf_sales",
    measures=[{"name": "amount", "sql_type": "numeric", "nullable": False}],
    dimensions={
        "name": "data",
        "paths": [
            {"name": "product", "json_path": "data->>'product'", "data_type": "text"}
        ],
    },
    denormalized_filters=[
        {"name": "user_id", "sql_type": "bigint", "indexed": True},
        {"name": "created_at", "sql_type": "timestamptz", "indexed": True},
    ],
)

# Step 5: Export
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
