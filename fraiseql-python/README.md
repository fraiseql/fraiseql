# FraiseQL v2 - Python Schema Authoring

**Python decorators for authoring FraiseQL schemas**

This package provides Python decorators to define GraphQL schemas that are compiled by the FraiseQL Rust engine.

## Architecture

```
Python Decorators → schema.json → fraiseql-cli compile → schema.compiled.json → Rust Runtime
```

**Important**: This package is for **schema authoring only**. It does NOT provide runtime execution.
The compiled schema is executed by the standalone Rust server.

## Installation

```bash
pip install fraiseql
```

## Quick Start

```python
import fraiseql

# Define a GraphQL type
@fraiseql.type
class User:
    id: int
    name: str
    email: str
    created_at: str

# Define a query
@fraiseql.query(sql_source="v_user")
def users(limit: int = 10) -> list[User]:
    """Get all users with pagination."""
    pass

# Define a mutation
@fraiseql.mutation(sql_source="fn_create_user", operation="CREATE")
def create_user(name: str, email: str) -> User:
    """Create a new user."""
    pass

# Export schema to JSON
if __name__ == "__main__":
    fraiseql.export_schema("schema.json")
```

## Compile Schema

```bash
# Compile schema.json to optimized schema.compiled.json
fraiseql-cli compile schema.json -o schema.compiled.json

# Start server with compiled schema
fraiseql-server --schema schema.compiled.json
```

## Features

- **Type-safe**: Python type hints map to GraphQL types
- **Database-backed**: Queries map to SQL views, mutations to functions
- **Compile-time**: All validation happens at compile time, zero runtime overhead
- **No FFI**: Pure JSON output, no Python-Rust bindings needed
- **Analytics**: Fact tables and aggregate queries for OLAP workloads

## Analytics / Fact Tables

FraiseQL supports high-performance analytics via fact tables:

```python
import fraiseql

# Define a fact table
@fraiseql.fact_table(
    table_name="tf_sales",
    measures=["revenue", "quantity", "cost"],
    dimension_paths=[
        {"name": "category", "json_path": "data->>'category'", "data_type": "text"},
        {"name": "region", "json_path": "data->>'region'", "data_type": "text"}
    ]
)
@fraiseql.type
class Sale:
    id: int
    revenue: float  # Measure (aggregatable)
    quantity: int   # Measure
    cost: float     # Measure
    customer_id: str  # Denormalized filter (indexed)
    occurred_at: str  # Denormalized filter (indexed)

# Define an aggregate query
@fraiseql.aggregate_query(
    fact_table="tf_sales",
    auto_group_by=True,
    auto_aggregates=True
)
@fraiseql.query
def sales_aggregate() -> list[dict]:
    """Aggregate sales with flexible grouping and filtering."""
```

This generates a GraphQL query that supports:

- **GROUP BY**: Dimensions (`category`, `region`) and temporal buckets (`occurred_at_day`, `occurred_at_month`)
- **Aggregates**: `count`, `revenue_sum`, `revenue_avg`, `quantity_sum`, etc.
- **WHERE**: Pre-aggregation filters (`customer_id`, `occurred_at` range)
- **HAVING**: Post-aggregation filters (`revenue_sum_gt: 1000`)
- **ORDER BY**: Any aggregate or dimension
- **LIMIT/OFFSET**: Pagination

### Fact Table Pattern

```sql
-- Table name starts with tf_ (table fact)
CREATE TABLE tf_sales (
    id BIGSERIAL PRIMARY KEY,
    -- Measures: Numeric columns for fast aggregation
    revenue DECIMAL(10,2) NOT NULL,
    quantity INT NOT NULL,
    cost DECIMAL(10,2) NOT NULL,
    -- Dimensions: JSONB column for flexible GROUP BY
    data JSONB NOT NULL,
    -- Denormalized filters: Indexed columns for fast WHERE
    customer_id UUID NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX ON tf_sales(customer_id);
CREATE INDEX ON tf_sales(occurred_at);
```

**Key Principles:**

- **Measures**: SQL columns (numeric types) for fast aggregation
- **Dimensions**: JSONB `data` column for flexible grouping
- **Denormalized Filters**: Indexed SQL columns for fast WHERE clauses
- **No Joins**: All dimensional data denormalized at ETL time

## Type Mapping

| Python Type | GraphQL Type |
|-------------|--------------|
| `int` | `Int` |
| `float` | `Float` |
| `str` | `String` |
| `bool` | `Boolean` |
| `list[T]` | `[T]` |
| `T \| None` | `T` (nullable) |
| Custom class | Object type |

## Documentation

Full documentation: <https://fraiseql.readthedocs.io>

## License

MIT
