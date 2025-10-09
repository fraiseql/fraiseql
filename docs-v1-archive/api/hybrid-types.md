# Hybrid Types API Reference

## `register_type_for_view()`

Register a type for a database view/table with optional metadata for hybrid table support.

```python
def register_type_for_view(
    view_name: str,
    type_class: type,
    table_columns: set[str] | None = None,
    has_jsonb_data: bool | None = None
) -> None
```

### Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `view_name` | `str` | - | Database table or view name |
| `type_class` | `type` | - | Python class decorated with `@fraiseql.type` |
| `table_columns` | `set[str] \| None` | `None` | Set of actual database column names |
| `has_jsonb_data` | `bool \| None` | `None` | Whether table has a JSONB 'data' column |

### Examples

#### Basic Registration
```python
register_type_for_view("products", Product)
```

#### Hybrid Table Registration
```python
register_type_for_view(
    "products",
    Product,
    table_columns={'id', 'name', 'status', 'is_active', 'data'},
    has_jsonb_data=True
)
```

### Performance Impact

| Registration Type | Field Detection Time | Database Queries |
|------------------|---------------------|------------------|
| With metadata | 0.4 μs | None |
| Without metadata | 0.4 μs + introspection | One-time per table |

## `@hybrid_type` Decorator

**Note**: Future enhancement - decorator for automatic registration.

```python
from fraiseql.decorators.hybrid_type import hybrid_type

@fraiseql.type
@hybrid_type(
    sql_source="products",
    regular_columns={'id', 'status', 'is_active'},
    has_jsonb_data=True
)
class Product:
    # Type definition...
```

## Internal APIs

### `FraiseQLRepository._should_use_jsonb_path_sync()`

Internal method for determining whether to use JSONB path or direct column access.

```python
def _should_use_jsonb_path_sync(self, view_name: str, field_name: str) -> bool
```

**Returns**: `True` if field should use JSONB path (`data->>'field'`), `False` for direct column access.

### Cache Management

FraiseQL maintains several internal caches for performance:

- `_field_path_cache`: Field-level routing decisions
- `_table_has_jsonb`: Table-level JSONB detection
- `_introspected_columns`: Database introspection results

## Error Handling

### Common Errors

#### `NotImplementedError: Type registry lookup failed`
```python
# Cause: Type not registered
register_type_for_view("my_table", MyType)
```

#### `UndefinedColumn: column "field" does not exist`
```python
# Cause: Field incorrectly classified as regular column
register_type_for_view(
    "my_table",
    MyType,
    table_columns={'id', 'status', 'data'},  # Don't include JSONB fields
    has_jsonb_data=True
)
```

### Debug Information

Enable debug logging to troubleshoot field classification:

```python
import logging
logging.getLogger('fraiseql.db').setLevel(logging.DEBUG)
```

## Migration Guide

### From Pure JSONB Tables

#### Before
```python
# All fields stored in JSONB
register_type_for_view("products", Product)
```

#### After
```python
# Extract common filters to regular columns
register_type_for_view(
    "products",
    Product,
    table_columns={'id', 'status', 'is_active', 'data'},
    has_jsonb_data=True
)
```

### Database Schema Changes

```sql
-- Add regular columns for commonly-filtered fields
ALTER TABLE products
ADD COLUMN status TEXT,
ADD COLUMN is_active BOOLEAN;

-- Populate from JSONB
UPDATE products SET
    status = data->>'status',
    is_active = (data->>'is_active')::boolean;

-- Remove from JSONB to avoid duplication
UPDATE products SET data = data - 'status' - 'is_active';

-- Add indexes for performance
CREATE INDEX idx_products_status ON products(status);
CREATE INDEX idx_products_active ON products(is_active);
```

## Type Safety

### Column Set Validation

Ensure `table_columns` includes all actual database columns:

```python
# ✅ Correct - includes all columns
table_columns={'id', 'name', 'status', 'data'}

# ❌ Incorrect - missing 'data' column
table_columns={'id', 'name', 'status'}

# ❌ Incorrect - includes JSONB fields
table_columns={'id', 'name', 'status', 'brand', 'data'}
```

### Field Classification Rules

| Field Location | `table_columns` | Result |
|----------------|-----------------|---------|
| Regular column | Included | Direct access: `WHERE field = value` |
| JSONB field | Not included | JSONB path: `WHERE data->>'field' = value` |
| Regular column | Not included | ⚠️ Incorrectly uses JSONB path |
| JSONB field | Included | ❌ Column does not exist error |

## Performance Monitoring

### Metrics to Track

```python
# Field detection time (should be < 1μs with metadata)
start = time.perf_counter()
repo._should_use_jsonb_path_sync("products", "status")
detection_time = time.perf_counter() - start

# Cache hit rates
cache_size = len(repo._field_path_cache)
introspection_calls = len(repo._introspected_columns)
```

### Memory Usage

```python
import sys

# Metadata memory per table (~1KB)
metadata_size = sys.getsizeof({
    'columns': {'id', 'status', 'data'},
    'has_jsonb_data': True
})

# Total for N tables
total_memory = metadata_size * num_tables
```
