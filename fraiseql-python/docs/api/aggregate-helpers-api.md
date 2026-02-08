# Aggregate Helpers API Reference

**Module**: `fraiseql.sql.aggregate_helpers`
**Version**: 1.9.0+
**Status**: Stable

---

## Quick Reference

```python
from fraiseql.sql.aggregate_helpers import (
    build_aggregate_expression,
    build_aggregate_dict,
    get_required_cast,
)
```

---

## Functions

### `build_aggregate_expression()`

Build a single aggregate SQL expression with proper type casting.

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

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `function` | `str` | Required | Aggregate function name (e.g., "SUM", "AVG", "COUNT") |
| `field` | `str \| None` | `None` | Field name to aggregate (None for COUNT(*)) |
| `is_jsonb` | `bool` | `True` | Whether field is in JSONB column |
| `jsonb_column` | `str` | `"data"` | Name of JSONB column |
| `distinct` | `bool` | `False` | Whether to use DISTINCT |

**Returns:** `str` - SQL expression with proper casting

**Raises:**
- `ValueError` - If function requires a field but none provided

**Examples:**

```python
# COUNT(*)
build_aggregate_expression("COUNT")
# → "COUNT(*)"

# SUM with JSONB casting
build_aggregate_expression("SUM", "amount")
# → "SUM((data->'amount')::numeric)"

# AVG without casting (SQL column)
build_aggregate_expression("AVG", "price", is_jsonb=False)
# → "AVG(price)"

# COUNT DISTINCT
build_aggregate_expression("COUNT", "user_id", distinct=True)
# → "COUNT(DISTINCT data->'user_id')"
```

---

### `build_aggregate_dict()`

Build multiple aggregate expressions at once.

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

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `aggregates` | `dict[str, str \| dict]` | Required | Mapping of aliases to expressions or specs |
| `is_jsonb` | `bool` | `True` | Whether fields are in JSONB column |
| `jsonb_column` | `str` | `"data"` | Name of JSONB column |

**Aggregate Spec Format:**

```python
{
    "function": str,      # Required: "SUM", "AVG", "COUNT", etc.
    "field": str | None,  # Required for most (None for COUNT(*))
    "distinct": bool,     # Optional: Use DISTINCT (default: False)
}
```

**Returns:** `dict[str, str]` - Mapping of aliases to SQL expressions

**Raises:**
- `TypeError` - If aggregate spec is not str or dict

**Examples:**

```python
# Structured specs
build_aggregate_dict({
    "total": {"function": "COUNT"},
    "sum_amount": {"function": "SUM", "field": "amount"},
})
# → {
#     "total": "COUNT(*)",
#     "sum_amount": "SUM((data->'amount')::numeric)"
# }

# Raw SQL strings
build_aggregate_dict({
    "custom": "MAX(created_at) - MIN(created_at)"
})
# → {"custom": "MAX(created_at) - MIN(created_at)"}

# Mixed
build_aggregate_dict({
    "count": {"function": "COUNT"},
    "custom": "SUM(CASE WHEN status = 'active' THEN 1 ELSE 0 END)",
})
```

---

### `get_required_cast()`

Get the required PostgreSQL cast type for an aggregate function.

**Signature:**
```python
def get_required_cast(function: str) -> Literal["numeric", "timestamp", "text", "none"]
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `function` | `str` | Aggregate function name (case-insensitive) |

**Returns:** `Literal["numeric", "timestamp", "text", "none"]` - Required cast type

**Examples:**

```python
get_required_cast("SUM")        # → "numeric"
get_required_cast("AVG")        # → "numeric"
get_required_cast("STDDEV")     # → "numeric"
get_required_cast("COUNT")      # → "none"
get_required_cast("MIN")        # → "none"
get_required_cast("MAX")        # → "none"
get_required_cast("STRING_AGG") # → "text"
get_required_cast("ARRAY_AGG")  # → "none"
```

---

## Type Casting Reference

### Aggregate Functions by Cast Requirement

#### Numeric Cast Required (`::numeric`)

Functions that require numeric casting for JSONB fields:

- `SUM` - Sum of values
- `AVG` - Average of values
- `STDDEV` - Standard deviation
- `VARIANCE` - Variance
- `PERCENTILE_CONT` - Continuous percentile

**Example:**
```python
build_aggregate_expression("SUM", "amount", is_jsonb=True)
# → "SUM((data->'amount')::numeric)"
```

#### Text Cast Required (`::text`)

Functions that require text casting for JSONB fields:

- `STRING_AGG` - String aggregation with delimiter

**Example:**
```python
build_aggregate_expression("STRING_AGG", "name", is_jsonb=True)
# → "STRING_AGG((data->'name')::text)"
```

#### No Cast Required (`none`)

Functions that work without casting:

- `COUNT` - Count of rows or non-null values
- `MIN` - Minimum value (works with any comparable type)
- `MAX` - Maximum value (works with any comparable type)
- `ARRAY_AGG` - Array aggregation (preserves type)
- `JSON_AGG` - JSON aggregation (preserves type)
- `JSONB_AGG` - JSONB aggregation (preserves type)

**Example:**
```python
build_aggregate_expression("MAX", "created_at", is_jsonb=True)
# → "MAX(data->'created_at')"
```

---

## Constants

### `AGGREGATE_CAST_REQUIREMENTS`

Mapping of aggregate functions to required cast types.

```python
AGGREGATE_CAST_REQUIREMENTS: dict[str, CastType] = {
    "COUNT": "none",
    "SUM": "numeric",
    "AVG": "numeric",
    "MIN": "none",
    "MAX": "none",
    "STDDEV": "numeric",
    "VARIANCE": "numeric",
    "PERCENTILE_CONT": "numeric",
    "ARRAY_AGG": "none",
    "JSON_AGG": "none",
    "JSONB_AGG": "none",
    "STRING_AGG": "text",
}
```

---

## Type Definitions

### `AggregateFunction`

```python
AggregateFunction = Literal[
    "COUNT", "SUM", "AVG", "MIN", "MAX",
    "STDDEV", "VARIANCE", "PERCENTILE_CONT",
    "ARRAY_AGG", "JSON_AGG", "JSONB_AGG", "STRING_AGG"
]
```

### `CastType`

```python
CastType = Literal["numeric", "timestamp", "text", "none"]
```

---

## Usage Examples

### Basic Aggregation

```python
from fraiseql.sql.aggregate_helpers import build_aggregate_dict

aggregates = build_aggregate_dict({
    "order_count": {"function": "COUNT"},
    "total_revenue": {"function": "SUM", "field": "amount"},
    "avg_order_value": {"function": "AVG", "field": "amount"},
})

result = await db.aggregate("v_orders", aggregations=aggregates)
# {
#     "order_count": 150,
#     "total_revenue": 125000.50,
#     "avg_order_value": 833.34
# }
```

### With WHERE Clause

```python
aggregates = build_aggregate_dict({
    "completed_count": {"function": "COUNT"},
    "completed_revenue": {"function": "SUM", "field": "amount"},
}, is_jsonb=True)

result = await db.aggregate(
    "v_orders",
    aggregations=aggregates,
    where={"status": {"eq": "completed"}}
)
```

### Distinct Counts

```python
aggregates = build_aggregate_dict({
    "total_orders": {"function": "COUNT"},
    "unique_customers": {
        "function": "COUNT",
        "field": "customer_id",
        "distinct": True
    },
}, is_jsonb=True)

result = await db.aggregate("v_orders", aggregations=aggregates)
```

### Mixed SQL and JSONB

```python
# Hybrid table with both SQL columns and JSONB data
sql_aggregates = build_aggregate_dict({
    "count": {"function": "COUNT"},
}, is_jsonb=False)

jsonb_aggregates = build_aggregate_dict({
    "sum_amount": {"function": "SUM", "field": "amount"},
    "avg_rating": {"function": "AVG", "field": "rating"},
}, is_jsonb=True)

result = await db.aggregate(
    "v_hybrid_table",
    aggregations={**sql_aggregates, **jsonb_aggregates}
)
```

### Custom SQL Expressions

```python
aggregates = {
    # Use helpers for standard aggregates
    **build_aggregate_dict({
        "count": {"function": "COUNT"},
        "sum_amount": {"function": "SUM", "field": "amount"},
    }, is_jsonb=True),

    # Use raw SQL for complex expressions
    "revenue_range": "MAX((data->'amount')::numeric) - MIN((data->'amount')::numeric)",
    "active_ratio": "SUM(CASE WHEN status = 'active' THEN 1.0 ELSE 0.0 END) / COUNT(*)",
}

result = await db.aggregate("v_orders", aggregations=aggregates)
```

---

## Performance Tips

### 1. Create Functional Indexes

For frequently-aggregated JSONB fields:

```sql
-- Numeric fields
CREATE INDEX idx_orders_amount_numeric
ON orders (((data->'amount')::numeric));

-- Temporal fields
CREATE INDEX idx_orders_created_at
ON orders (((data->>'created_at')::timestamp));
```

### 2. Use Appropriate Data Types

```python
# ✅ GOOD: Numeric cast for numbers
build_aggregate_expression("SUM", "amount", is_jsonb=True)
# → Uses functional index on ((data->'amount')::numeric)

# ❌ BAD: Text extraction for numbers
"SUM((data->>'amount')::numeric)"
# → Cannot use index efficiently
```

### 3. DISTINCT is Expensive

```python
# ✅ EFFICIENT: DISTINCT on small, indexed fields
build_aggregate_expression("COUNT", "status", distinct=True)

# ⚠️ SLOW: DISTINCT on large text fields
build_aggregate_expression("COUNT", "description", distinct=True)
```

---

## Error Handling

### Missing Required Field

```python
try:
    build_aggregate_expression("SUM")  # Missing field
except ValueError as e:
    print(e)  # "SUM requires a field argument"
```

### Invalid Spec Type

```python
try:
    build_aggregate_dict({"invalid": 123})
except TypeError as e:
    print(e)  # "Invalid aggregate spec for 'invalid': must be str or dict"
```

---

## See Also

- [Aggregate Helpers User Guide](../features/aggregation-helpers.md)
- [Repository API Reference](./repository.md)
- [PostgreSQL Aggregate Functions](https://www.postgresql.org/docs/current/functions-aggregate.html)

---

**Last Updated**: 2026-01-12
**Module Version**: 1.9.0+
