# Hybrid Table Support

FraiseQL supports **hybrid tables** - database tables that contain both regular SQL columns and JSONB data columns. This architecture is common in applications that need:

- Fast filtering on commonly-queried fields (using regular columns)
- Flexible storage for variable or nested data (using JSONB)

## Architecture

A hybrid table typically looks like:

```sql
CREATE TABLE products (
    -- Regular SQL columns (optimized for filtering)
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    status TEXT NOT NULL,
    is_active BOOLEAN DEFAULT true,
    category_id UUID,
    created_date DATE,

    -- JSONB column (flexible data storage)
    data JSONB
);
```

## Type Definition

Define your FraiseQL type normally:

```python
import fraiseql

@fraiseql.type
class Product:
    # Fields that map to regular columns
    id: UUID
    name: str
    status: str
    is_active: bool
    category_id: UUID | None
    created_date: date | None

    # Fields that come from JSONB data
    brand: str | None
    color: str | None
    specifications: dict | None
```

## Registration with Metadata

For optimal performance, register your type with column metadata to avoid runtime database introspection:

```python
from fraiseql.db import register_type_for_view

register_type_for_view(
    "products",
    Product,
    table_columns={
        'id', 'name', 'status', 'is_active',
        'category_id', 'created_date', 'data'
    },
    has_jsonb_data=True
)
```

### Parameters

- **`table_columns`**: Set of actual database column names
- **`has_jsonb_data`**: Whether the table has a JSONB `data` column

## Filtering Behavior

FraiseQL automatically generates the correct SQL based on field type:

### Regular Column Filtering
```python
# GraphQL: where: { isActive: { eq: true } }
# Generated SQL: WHERE is_active = true
```

### JSONB Field Filtering
```python
# GraphQL: where: { brand: { eq: "TechCorp" } }
# Generated SQL: WHERE data->>'brand' = 'TechCorp'
```

### Mixed Filtering
```python
# GraphQL: where: {
#   isActive: { eq: true },
#   brand: { eq: "TechCorp" }
# }
# Generated SQL: WHERE is_active = true AND data->>'brand' = 'TechCorp'
```

## Performance

### With Metadata Registration
- **Field detection**: 0.4 microseconds per field
- **No database queries** during filtering
- **Memory overhead**: ~1KB per table

### Without Metadata (Fallback)
- Falls back to heuristic-based detection
- May require one-time database introspection
- Less accurate field classification

## Best Practices

### 1. Register Types at Import Time
```python
# In your models.py or types.py
register_type_for_view(
    "products",
    Product,
    table_columns={'id', 'status', 'is_active', 'data'},
    has_jsonb_data=True
)
```

### 2. Use Regular Columns for Common Filters
Store frequently-filtered fields as regular columns:
- IDs and foreign keys
- Status and state fields
- Boolean flags
- Dates used in range queries

### 3. Use JSONB for Flexible Data
Store variable or nested data in JSONB:
- User preferences
- Configuration objects
- Metadata and tags
- Nested relationships

### 4. Index Appropriately
```sql
-- Index regular columns for fast filtering
CREATE INDEX idx_products_status ON products(status);
CREATE INDEX idx_products_active ON products(is_active);

-- Index JSONB fields if needed
CREATE INDEX idx_products_brand ON products USING GIN ((data->>'brand'));
```

## Migration from Pure JSONB

If you're migrating from a pure JSONB approach:

### Before (Pure JSONB)
```sql
CREATE TABLE products (
    id UUID PRIMARY KEY,
    data JSONB
);
```

### After (Hybrid)
```sql
-- Extract commonly-filtered fields to columns
ALTER TABLE products
ADD COLUMN status TEXT,
ADD COLUMN is_active BOOLEAN;

-- Populate from existing JSONB data
UPDATE products SET
    status = data->>'status',
    is_active = (data->>'is_active')::boolean;

-- Add indexes
CREATE INDEX idx_products_status ON products(status);
CREATE INDEX idx_products_active ON products(is_active);
```

## Troubleshooting

### Fields Not Filtering Correctly

Check your registration metadata:
```python
# Ensure all regular columns are listed
register_type_for_view(
    "my_table",
    MyType,
    table_columns={'id', 'status', 'created_at', 'data'},  # Include 'data'!
    has_jsonb_data=True
)
```

### Performance Issues

1. **Register with metadata** to avoid runtime introspection
2. **Use regular columns** for frequently-filtered fields
3. **Add appropriate indexes** on both regular and JSONB columns

### Debugging

Enable debug logging to see generated SQL:
```python
import logging
logging.getLogger('fraiseql.db').setLevel(logging.DEBUG)
```

## Examples

### E-commerce Product Catalog
```python
@fraiseql.type
class Product:
    # Fast filtering (regular columns)
    id: UUID
    status: str  # published, draft, archived
    is_active: bool
    category_id: UUID
    price: Decimal

    # Flexible data (JSONB)
    brand: str
    specifications: dict
    variants: list[dict]
```

### User Profiles
```python
@fraiseql.type
class UserProfile:
    # Core identity (regular columns)
    id: UUID
    email: str
    is_verified: bool
    created_date: date

    # Preferences (JSONB)
    settings: dict
    preferences: dict
    metadata: dict
```

### Content Management
```python
@fraiseql.type
class Article:
    # Publishing workflow (regular columns)
    id: UUID
    status: str  # draft, review, published
    author_id: UUID
    published_date: date

    # Content (JSONB)
    title: str
    content: str
    tags: list[str]
    seo_metadata: dict
```
