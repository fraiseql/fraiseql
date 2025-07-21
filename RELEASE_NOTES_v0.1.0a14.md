# FraiseQL v0.1.0a14 Release Notes

## Overview

This release introduces a **breaking change** that aligns FraiseQL with the PrintOptim architecture pattern. Type instantiation now exclusively uses a JSONB `data` column, providing a cleaner separation between data storage and access control.

## Breaking Changes

### JSONB Data Column Pattern (Required)

All tables/views must now have a `data` column containing the complete object representation:

```sql
CREATE TABLE your_table (
    id uuid,              -- For primary key
    tenant_id uuid,       -- For access control
    other_id uuid,        -- For filtering/joins

    data jsonb,           -- REQUIRED: Complete object data

    created_at timestamptz,
    updated_by uuid
);
```

### What Changed

**Before (v0.1.0a13):**
- FraiseQL could instantiate from entire row or from nested JSONB
- Backward compatibility with multiple patterns
- Complex logic to detect instantiation pattern

**Now (v0.1.0a14):**
- FraiseQL ONLY instantiates from the `data` column
- Single, consistent pattern across all types
- Cleaner, simpler implementation

## Migration Guide

### 1. Update Your Database Views

Ensure all views have a `data` column:

```sql
-- Example: Update your view to include data column
CREATE OR REPLACE VIEW your_view AS
SELECT
    t.id,
    t.tenant_id,
    t.other_columns,
    jsonb_build_object(
        'id', t.id,
        'name', t.name,
        'nested_object', t.nested_data,
        -- ... all fields for your type
    ) as data
FROM your_table t;
```

### 2. Simplify Your Type Definitions

No more dual-type pattern needed:

```python
# Single type definition
@fraise_type
class YourType:
    id: UUID
    name: str
    nested_object: Optional[NestedType]
    # All fields from your JSONB data
```

### 3. Usage Remains the Same

```python
# Development mode - returns typed objects
repo = FraiseQLRepository(pool, {"mode": "development"})
item = await repo.find_one("your_view", id=item_id)
print(item.name)  # Typed access

# Production mode - returns raw dicts
repo = FraiseQLRepository(pool, {"mode": "production"})
item = await repo.find_one("your_view", id=item_id)
print(item["data"]["name"])  # Dict access
```

## Benefits

- **Consistency**: One pattern for all types
- **Simplicity**: No complex detection logic
- **Clarity**: Clear separation between filtering columns and data
- **Performance**: Optimized for the single pattern
- **Alignment**: Matches PrintOptim architecture exactly

## Technical Details

- Removed all backward compatibility code
- Simplified `_instantiate_from_row()` method
- Cleaner codebase following KISS principle
- Better error messages when `data` column is missing

## Installation

```bash
pip install fraiseql==0.1.0a14
```

## Note

This is an alpha release with breaking changes. Please test thoroughly before using in production.
