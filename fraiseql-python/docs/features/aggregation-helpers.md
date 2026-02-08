# Aggregate SQL Helpers

**Version**: 1.9.0+
**Status**: Stable
**Module**: `fraiseql.sql.aggregate_helpers`

---

## Overview

FraiseQL provides helper functions for building type-safe SQL aggregate expressions, especially for **JSONB fields** which require explicit type casting in PostgreSQL.

### Why Use These Helpers?

PostgreSQL's JSONB type requires explicit casting for numeric and temporal operations:

```python
# ❌ BROKEN: PostgreSQL error on JSONB fields
"SUM(data->'amount')"  # ERROR: function sum(jsonb) does not exist

# ✅ CORRECT: Requires explicit cast
"SUM((data->'amount')::numeric)"
```

The aggregate helpers **automatically generate correct SQL** based on the aggregate function type.

---

## Quick Start

```python
from fraiseql.sql.aggregate_helpers import build_aggregate_expression

# Simple COUNT
build_aggregate_expression("COUNT")
# → "COUNT(*)"

# SUM with automatic casting for JSONB
build_aggregate_expression("SUM", "amount", is_jsonb=True)
# → "SUM((data->'amount')::numeric)"

# AVG without casting for SQL columns
build_aggregate_expression("AVG", "price", is_jsonb=False)
# → "AVG(price)"
```

---

## Core Functions

### `build_aggregate_expression()`

Generate a single aggregate SQL expression with proper type casting.

**Signature:**
```python
def build_aggregate_expression(
    function: str,
    field: str | None = None,
    *,
    is_jsonb: bool = True,
    jsonb_column: str = "data",
    distinct: bool = False,
) -> str
```

**Parameters:**
- `function` (str): Aggregate function name (e.g., "SUM", "AVG", "COUNT")
- `field` (str | None): Field name to aggregate (None for COUNT(*))
- `is_jsonb` (bool): Whether field is in JSONB column (default: True)
- `jsonb_column` (str): Name of JSONB column (default: "data")
- `distinct` (bool): Whether to use DISTINCT (default: False)

**Returns:** SQL expression string with proper casting

**Examples:**

```python
# Basic aggregates
build_aggregate_expression("COUNT")
# → "COUNT(*)"

build_aggregate_expression("COUNT", "id")
# → "COUNT(data->'id')"

build_aggregate_expression("COUNT", "id", distinct=True)
# → "COUNT(DISTINCT data->'id')"

# Numeric aggregates (auto-cast for JSONB)
build_aggregate_expression("SUM", "amount")
# → "SUM((data->'amount')::numeric)"

build_aggregate_expression("AVG", "price")
# → "AVG((data->'price')::numeric)"

build_aggregate_expression("STDDEV", "temperature")
# → "STDDEV((data->'temperature')::numeric)"

# Comparison aggregates (no cast needed)
build_aggregate_expression("MIN", "created_at")
# → "MIN(data->'created_at')"

build_aggregate_expression("MAX", "updated_at")
# → "MAX(data->'updated_at')"

# SQL columns (no JSONB casting)
build_aggregate_expression("SUM", "revenue", is_jsonb=False)
# → "SUM(revenue)"

# Custom JSONB column name
build_aggregate_expression("SUM", "total", jsonb_column="payload")
# → "SUM((payload->'total')::numeric)"
```

---

### `build_aggregate_dict()`

Build multiple aggregate expressions at once, supporting both structured specs and raw SQL.

**Signature:**
```python
def build_aggregate_dict(
    aggregates: dict[str, str | dict],
    *,
    is_jsonb: bool = True,
    jsonb_column: str = "data",
) -> dict[str, str]
```

**Parameters:**
- `aggregates` (dict): Mapping of aliases to expressions or structured specs
- `is_jsonb` (bool): Whether fields are in JSONB column (default: True)
- `jsonb_column` (str): Name of JSONB column (default: "data")

**Returns:** Dict mapping aliases to SQL expressions

**Examples:**

```python
from fraiseql.sql.aggregate_helpers import build_aggregate_dict

# Structured specs with auto-casting
build_aggregate_dict({
    "total": {"function": "COUNT"},
    "sum_amount": {"function": "SUM", "field": "amount"},
    "avg_price": {"function": "AVG", "field": "price"},
})
# → {
#     "total": "COUNT(*)",
#     "sum_amount": "SUM((data->'amount')::numeric)",
#     "avg_price": "AVG((data->'price')::numeric)"
# }

# Mix structured and raw SQL
build_aggregate_dict({
    "total": {"function": "COUNT"},
    "revenue": {"function": "SUM", "field": "amount"},
    "custom_metric": "MAX(created_at) - MIN(created_at)",  # Raw SQL
})
# → {
#     "total": "COUNT(*)",
#     "revenue": "SUM((data->'amount')::numeric)",
#     "custom_metric": "MAX(created_at) - MIN(created_at)"
# }

# DISTINCT aggregates
build_aggregate_dict({
    "unique_customers": {
        "function": "COUNT",
        "field": "customer_id",
        "distinct": True
    },
})
# → {"unique_customers": "COUNT(DISTINCT data->'customer_id')"}
```

---

### `get_required_cast()`

Determine the required PostgreSQL cast type for an aggregate function.

**Signature:**
```python
def get_required_cast(function: str) -> Literal["numeric", "timestamp", "text", "none"]
```

**Parameters:**
- `function` (str): Aggregate function name (case-insensitive)

**Returns:** Required cast type, or "none" if no cast needed

**Examples:**

```python
from fraiseql.sql.aggregate_helpers import get_required_cast

get_required_cast("SUM")      # → "numeric"
get_required_cast("AVG")      # → "numeric"
get_required_cast("STDDEV")   # → "numeric"
get_required_cast("COUNT")    # → "none"
get_required_cast("MIN")      # → "none"
get_required_cast("MAX")      # → "none"
get_required_cast("STRING_AGG")  # → "text"
```

---

## Type Casting Rules

### Numeric Aggregates

**Require `::numeric` cast for JSONB:**
- `SUM`
- `AVG`
- `STDDEV`
- `VARIANCE`
- `PERCENTILE_CONT`

**Example:**
```python
# JSONB field
build_aggregate_expression("SUM", "amount", is_jsonb=True)
# → "SUM((data->'amount')::numeric)"

# SQL column (no cast)
build_aggregate_expression("SUM", "amount", is_jsonb=False)
# → "SUM(amount)"
```

### Comparison Aggregates

**No cast needed** (work with any comparable type):
- `MIN`
- `MAX`

**Example:**
```python
# Works on dates, numbers, strings without casting
build_aggregate_expression("MAX", "created_at", is_jsonb=True)
# → "MAX(data->'created_at')"
```

### Counting Aggregates

**No cast needed:**
- `COUNT`

**Example:**
```python
build_aggregate_expression("COUNT")
# → "COUNT(*)"

build_aggregate_expression("COUNT", "id", is_jsonb=True)
# → "COUNT(data->'id')"
```

### Array/JSON Aggregates

**No cast needed** (preserve type):
- `ARRAY_AGG`
- `JSON_AGG`
- `JSONB_AGG`

**Example:**
```python
build_aggregate_expression("ARRAY_AGG", "tags", is_jsonb=True)
# → "ARRAY_AGG(data->'tags')"
```

### String Aggregates

**Require `::text` cast for JSONB:**
- `STRING_AGG`

**Example:**
```python
build_aggregate_expression("STRING_AGG", "category", is_jsonb=True)
# → "STRING_AGG((data->'category')::text)"
```

---

## Usage with Repository

### Option 1: Using `aggregate()` with Helpers

```python
from fraiseql.sql.aggregate_helpers import build_aggregate_dict

# Build type-safe aggregates
aggregates = build_aggregate_dict({
    "total": {"function": "COUNT"},
    "sum_amount": {"function": "SUM", "field": "amount"},
    "avg_amount": {"function": "AVG", "field": "amount"},
    "max_amount": {"function": "MAX", "field": "amount"},
}, is_jsonb=True)

# Execute with repository
result = await db.aggregate(
    "v_orders",
    aggregations=aggregates,
    where={"status": {"eq": "completed"}}
)

# Result:
# {
#     "total": 150,
#     "sum_amount": 125000.50,
#     "avg_amount": 833.34,
#     "max_amount": 2500.00
# }
```

### Option 2: Raw SQL (Advanced)

For complete control, pass raw SQL directly:

```python
result = await db.aggregate(
    "v_orders",
    aggregations={
        "total": "COUNT(*)",
        "revenue": "SUM((data->'amount')::numeric)",
        "custom": "SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END)",
    },
    where={"created_at": {"gte": "2026-01-01"}}
)
```

---

## Common Patterns

### Revenue Analytics

```python
from fraiseql.sql.aggregate_helpers import build_aggregate_dict

# Daily revenue summary
aggregates = build_aggregate_dict({
    "order_count": {"function": "COUNT"},
    "total_revenue": {"function": "SUM", "field": "amount"},
    "avg_order_value": {"function": "AVG", "field": "amount"},
    "min_order": {"function": "MIN", "field": "amount"},
    "max_order": {"function": "MAX", "field": "amount"},
    "revenue_stddev": {"function": "STDDEV", "field": "amount"},
}, is_jsonb=True)

stats = await db.aggregate("v_orders", aggregations=aggregates)
```

### Unique Counts

```python
# Count unique values
aggregates = build_aggregate_dict({
    "total_orders": {"function": "COUNT"},
    "unique_customers": {
        "function": "COUNT",
        "field": "customer_id",
        "distinct": True
    },
    "unique_products": {
        "function": "COUNT",
        "field": "product_id",
        "distinct": True
    },
}, is_jsonb=True)

metrics = await db.aggregate("v_orders", aggregations=aggregates)
```

### Mixed JSONB and SQL Columns

```python
# Hybrid table with both SQL columns and JSONB data
aggregates = {
    # SQL columns (no casting)
    **build_aggregate_dict({
        "count": {"function": "COUNT"},
    }, is_jsonb=False),

    # JSONB columns (with casting)
    **build_aggregate_dict({
        "sum_amount": {"function": "SUM", "field": "amount"},
        "avg_rating": {"function": "AVG", "field": "rating"},
    }, is_jsonb=True),
}

result = await db.aggregate("v_hybrid_table", aggregations=aggregates)
```

---

## Performance Considerations

### Automatic Casting Overhead

Type casting has minimal overhead but can affect index usage:

```python
# ✅ GOOD: Can use functional index on ((data->'amount')::numeric)
"SUM((data->'amount')::numeric)"

# ❌ LESS EFFICIENT: Cannot use index without cast
"SUM(data->>'amount')"  # Returns text, not numeric
```

**Recommendation**: Create functional indexes for frequently-aggregated JSONB fields:

```sql
CREATE INDEX idx_orders_amount_numeric
ON orders (((data->'amount')::numeric));
```

### Large Aggregations

For very large datasets, consider:
1. **Materialized views** with pre-computed aggregates
2. **Partial indexes** on filtered aggregates
3. **Parallel aggregation** (PostgreSQL 9.6+)

---

## Error Handling

### Missing Field

```python
# ❌ ERROR: SUM requires a field
build_aggregate_expression("SUM")
# → ValueError: SUM requires a field argument

# ✅ CORRECT
build_aggregate_expression("SUM", "amount")
```

### Invalid Spec Type

```python
# ❌ ERROR: Invalid spec type
build_aggregate_dict({
    "invalid": 123  # Not a string or dict
})
# → TypeError: Invalid aggregate spec for 'invalid': must be str or dict

# ✅ CORRECT
build_aggregate_dict({
    "valid": {"function": "COUNT"}
})
```

---

## Advanced Topics

### Custom Aggregate Functions

For PostgreSQL extensions or custom aggregates:

```python
# Use raw SQL for custom functions
aggregates = {
    "median_price": "PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY (data->'price')::numeric)",
    "p95_latency": "PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY (data->'latency_ms')::numeric)",
    # Standard aggregates still use helpers
    **build_aggregate_dict({
        "count": {"function": "COUNT"},
        "avg_price": {"function": "AVG", "field": "price"},
    }, is_jsonb=True)
}
```

### Conditional Aggregates

PostgreSQL 9.4+ supports `FILTER (WHERE ...)` clauses:

```python
# Currently requires raw SQL (Phase 3 will add helper support)
aggregates = {
    "total": "COUNT(*)",
    "completed": "COUNT(*) FILTER (WHERE status = 'completed')",
    "cancelled": "COUNT(*) FILTER (WHERE status = 'cancelled')",
    "avg_completed": "AVG((data->'amount')::numeric) FILTER (WHERE status = 'completed')",
}

result = await db.aggregate("v_orders", aggregations=aggregates)
```

---

## Roadmap

### Phase 1 (Current) ✅
- Table-wide aggregates
- JSONB type casting
- WHERE clause filtering

### Phase 2 (Planned)
- GROUP BY support
- HAVING clause
- Temporal bucketing

### Phase 3 (Planned)
- ARRAY_AGG with ORDER BY
- STRING_AGG with delimiter
- Statistical functions (STDDEV, VARIANCE, PERCENTILE)
- FILTER (WHERE ...) support in helpers

### Phase 4 (Planned)
- GraphQL auto-generation for aggregate queries
- `<Type>Aggregate` result types
- `<Type>_aggregate` query fields

---

## API Reference

### Function Signatures

```python
def build_aggregate_expression(
    function: str,
    field: str | None = None,
    *,
    is_jsonb: bool = True,
    jsonb_column: str = "data",
    distinct: bool = False,
) -> str:
    """Build a properly-cast SQL aggregate expression."""

def build_aggregate_dict(
    aggregates: dict[str, str | dict],
    *,
    is_jsonb: bool = True,
    jsonb_column: str = "data",
) -> dict[str, str]:
    """Build multiple aggregate expressions with automatic type casting."""

def get_required_cast(function: str) -> Literal["numeric", "timestamp", "text", "none"]:
    """Get the required PostgreSQL cast type for an aggregate function."""
```

### Type Definitions

```python
# Structured aggregate spec
AggregateSpec = {
    "function": str,           # Required: "SUM", "AVG", "COUNT", etc.
    "field": str | None,       # Required for most functions (None for COUNT(*))
    "distinct": bool,          # Optional: Use DISTINCT (default: False)
}

# Aggregate input (flexible)
AggregateInput = str | AggregateSpec  # Raw SQL string or structured spec
```

---

## Best Practices

### 1. Use Helpers for JSONB Fields

```python
# ✅ RECOMMENDED: Use helpers for type safety
aggregates = build_aggregate_dict({
    "sum_amount": {"function": "SUM", "field": "amount"},
}, is_jsonb=True)

# ❌ FRAGILE: Raw SQL requires manual casting
aggregates = {
    "sum_amount": "SUM((data->'amount')::numeric)"  # Easy to forget cast
}
```

### 2. Mix Helpers and Raw SQL When Needed

```python
# ✅ BEST OF BOTH WORLDS
aggregates = {
    # Use helpers for simple cases
    **build_aggregate_dict({
        "count": {"function": "COUNT"},
        "sum_amount": {"function": "SUM", "field": "amount"},
    }, is_jsonb=True),

    # Use raw SQL for complex expressions
    "weighted_avg": "SUM((data->'amount')::numeric * (data->'weight')::numeric) / SUM((data->'weight')::numeric)",
}
```

### 3. Create Functional Indexes

```sql
-- For frequently-aggregated JSONB fields
CREATE INDEX idx_orders_amount
ON orders (((data->'amount')::numeric));

CREATE INDEX idx_orders_created_at
ON orders (((data->>'created_at')::timestamp));
```

### 4. Use DISTINCT Carefully

```python
# ✅ GOOD: DISTINCT on indexed column
build_aggregate_expression("COUNT", "customer_id", distinct=True)

# ⚠️ EXPENSIVE: DISTINCT on large text fields
build_aggregate_expression("COUNT", "description", distinct=True)  # Slow!
```

---

## See Also

- [FraiseQL Aggregation Guide](./aggregation-and-grouping.md) (Phase 2+)
- [Repository API Reference](../api/repository.md)
- [JSONB Best Practices](./jsonb-performance.md)
- [PostgreSQL Aggregate Functions](https://www.postgresql.org/docs/current/functions-aggregate.html)

---

**Last Updated**: 2026-01-12
**FraiseQL Version**: 1.9.0+
**Status**: Stable (Phase 1)
