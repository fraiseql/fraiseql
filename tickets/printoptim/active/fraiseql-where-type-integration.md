# FraiseQL Where Type Integration Guide

## Overview

FraiseQL now supports operator-based filtering using dynamically generated where types, similar to your existing `safe_create_where_type` implementation. This feature is available in FraiseQL v0.1.0a14 and later.

## Key Changes

### 1. Dynamic Where Type Generation

Instead of manually defining filter types, you can now use `safe_create_where_type` from FraiseQL:

```python
from fraiseql import fraise_type
from fraiseql.sql.where_generator import safe_create_where_type

@fraise_type
class Machine:
    id: UUID
    name: str
    status: str
    power_output: float
    created_at: datetime
    is_active: bool

# Generate where type automatically
MachineWhere = safe_create_where_type(Machine)
```

### 2. Repository Integration

The `FraiseQLRepository` now supports where types directly:

```python
@fraiseql.query
async def machines(info, where: MachineWhere | None = None) -> list[Machine]:
    db = info.context["db"]
    return await db.find("machine_view", where=where)
```

### 3. Supported Operators

- **All types**: `eq`, `neq`, `isnull`
- **Numeric**: `gt`, `gte`, `lt`, `lte`, `in`
- **String**: `contains`, `startswith`, `matches`, `in`
- **Date/DateTime**: `gt`, `gte`, `lt`, `lte`
- **Boolean**: `eq`, `neq`

### 4. Automatic Type Casting

FraiseQL automatically handles type casting for JSONB columns:
- Numeric comparisons: `(data->>'price')::numeric > 100`
- Date comparisons: `(data->>'created_at')::timestamp >= '2024-01-01'`
- Boolean comparisons: `(data->>'is_active')::boolean = true`

## Migration Steps

### Step 1: Update FraiseQL

```bash
pip install --upgrade fraiseql>=0.1.0a14
```

### Step 2: Replace Custom Filter Implementation

Replace your custom `filter.py` with FraiseQL's built-in support:

```python
# Old approach
from printoptim.entrypoints.api.utilities.filter import safe_create_where_type

# New approach
from fraiseql.sql.where_generator import safe_create_where_type
```

### Step 3: Update Query Implementations

Your existing GraphQL queries should work with minimal changes:

```python
# Before
@strawberry.field
async def tv_machines(
    self,
    info: Info,
    where: Optional[TvMachineWhereInput] = None,
) -> List[TvMachine]:
    # Custom filtering logic
    ...

# After
@fraiseql.query
async def tv_machines(
    info,
    where: TvMachineWhere | None = None,
) -> list[TvMachine]:
    db = info.context["db"]
    return await db.find("tv_machine_view", where=where)
```

### Step 4: Register Types for Development Mode

If using development mode, register your types:

```python
from fraiseql.db import register_type_for_view

register_type_for_view("tv_machine_view", TvMachine)
register_type_for_view("tv_allocation_view", TvAllocation)
```

## Example Usage

```python
# GraphQL query
query {
  tvMachines(
    where: {
      powerOutput: { gt: 100.0 }
      status: { eq: "running" }
      createdAt: { gte: "2024-01-01T00:00:00Z" }
    }
  ) {
    id
    name
    powerOutput
  }
}

# Python usage
where = TvMachineWhere()
where.power_output = {"gt": 100.0}
where.status = {"eq": "running"}
where.created_at = {"gte": datetime(2024, 1, 1)}

machines = await db.find("tv_machine_view", where=where)
```

## Benefits

1. **Reduced Code**: Remove custom filter implementation
2. **Type Safety**: Automatic type casting prevents SQL errors
3. **Performance**: Optimized SQL generation with proper parameterization
4. **Consistency**: Same filtering pattern across all types

## Testing

Test the integration with:

```python
import pytest
from fraiseql.sql.where_generator import safe_create_where_type

def test_where_type_generation():
    MachineWhere = safe_create_where_type(Machine)

    where = MachineWhere()
    where.power_output = {"gt": 100}
    where.status = {"eq": "running"}

    # Test SQL generation
    sql = where.to_sql()
    assert sql is not None
```

## Compatibility Notes

- Your existing GraphQL schema remains unchanged
- Frontend queries continue to work without modification
- The operator syntax (`{eq: value}`) is identical to your current implementation

## Next Steps

1. Test with a single query first
2. Gradually migrate other queries
3. Remove custom filter implementation once all queries are migrated
4. Consider using connection pooling for better performance

## Questions?

If you encounter any issues during migration, please create an issue with:
- The specific query causing problems
- The generated SQL (if available)
- Any error messages

The FraiseQL team is here to help ensure a smooth transition!
