# Response: JSONB Data Column Pattern Support in FraiseQL

## Great News! FraiseQL v0.1.0a13 Now Supports Your Pattern

Based on your feedback, we've updated FraiseQL to support the PrintOptim pattern where a JSONB `data` column contains the complete object representation while other columns are used for filtering and access control.

## How It Works Now

### Your Database Structure (Keep As-Is)

```sql
CREATE TABLE public.tv_allocation (
    id uuid NOT NULL,
    tenant_id uuid,           -- For multi-tenancy
    machine_id uuid,          -- For filtering/joins
    location_id uuid,         -- For filtering/joins
    -- Other columns for access control and filtering...

    data jsonb,               -- Complete object representation
    last_updated timestamptz,
    updated_by uuid
);
```

### Your Type Definition (Single Type!)

```python
from fraiseql import fraise_type, fraise_field
from uuid import UUID
from datetime import date
from typing import Optional

@fraise_type
class Allocation:
    """Allocation type - all fields from the JSONB data column."""
    id: UUID
    identifier: str
    machine_id: Optional[UUID]
    location_id: Optional[UUID]
    valid_from: date
    valid_until: Optional[date]
    is_past: bool
    is_current: bool
    is_future: bool
    is_reserved: bool
    is_stock: bool
    notes: Optional[str]
    notes_contact: Optional[str]
    is_provisionnal: bool = fraise_field(default=False)

    # Nested objects - automatically instantiated
    machine: Optional[Machine]
    location: Optional[Location]
    organizational_unit: Optional[OrganizationalUnit]
    network_configuration: Optional[NetworkConfiguration]
```

### Usage

```python
# Development mode
repo = FraiseQLRepository(pool, {"mode": "development"})
allocation = await repo.find_one("tv_allocation", tenant_id=tenant_id, id=allocation_id)

# FraiseQL automatically detects the 'data' column and instantiates from it!
print(allocation.identifier)  # "ALLOC-001"
print(allocation.machine.name)  # Machine object from nested data
print(allocation.location.building)  # Location object from nested data

# Production mode - returns raw dict
repo = FraiseQLRepository(pool, {"mode": "production"})
allocation = await repo.find_one("tv_allocation", tenant_id=tenant_id, id=allocation_id)
print(allocation["data"]["identifier"])  # Dict access
```

## What Changed in FraiseQL

FraiseQL now automatically detects when a row has a `data` or `json_data` column containing a dictionary. When found, it instantiates the type from that column only, ignoring the other columns (which are meant for filtering/access control).

The detection is automatic:
1. If row has `data` column with dict → instantiate from `data`
2. If row has `json_data` column with dict → instantiate from `json_data`
3. Otherwise → instantiate from entire row (backward compatibility)

## Key Benefits

1. **Single Type Definition**: No need for separate `Allocation` and `AllocationData` types
2. **Matches Your Architecture**: Works exactly like PrintOptim's pattern
3. **Clean Separation**: Database columns for filtering, JSONB for data
4. **Automatic Detection**: No configuration needed

## Migration

No migration needed! Your existing database structure and queries work perfectly with this pattern. Just ensure your FraiseQL types match the structure within your JSONB `data` column.

## Example Query

Your existing queries continue to work:

```python
# The query uses columns for filtering
query = """
    SELECT * FROM tv_allocation
    WHERE tenant_id = $1
    AND is_current = true
    AND machine_id = $2
"""
# Returns rows with 'data' column containing full objects

# FraiseQL automatically instantiates from the 'data' column
allocations = await repo.find("tv_allocation",
    tenant_id=tenant_id,
    is_current=True,
    machine_id=machine_id
)
```

## Summary

- Keep your database structure with the JSONB `data` column
- Define a single `Allocation` type matching the JSONB structure
- FraiseQL automatically handles instantiation from the `data` column
- Other columns remain available for filtering and access control

This matches the PrintOptim architecture where only the JSONB column contains the data needed for type instantiation!
