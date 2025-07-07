# Response: JSONB Data Column Pattern in FraiseQL

## FraiseQL Now Uses Your Pattern!

FraiseQL v0.1.0a13 has been updated to use the same architecture as PrintOptim: all type instantiation comes from a JSONB `data` column.

## The Pattern

### Database Structure
```sql
CREATE TABLE tv_allocation (
    id uuid,              -- For primary key
    tenant_id uuid,       -- For access control
    machine_id uuid,      -- For filtering/joins
    location_id uuid,     -- For filtering/joins

    data jsonb,           -- The ONLY source for object instantiation

    last_updated timestamptz,
    updated_by uuid
);
```

### Type Definition
```python
@fraise_type
class Allocation:
    """Single type matching the JSONB data structure."""
    id: UUID
    identifier: str
    machine_id: Optional[UUID]
    location_id: Optional[UUID]
    valid_from: date
    valid_until: Optional[date]
    is_current: bool
    notes: Optional[str]

    # Nested objects
    machine: Optional[Machine]
    location: Optional[Location]
    organizational_unit: Optional[OrganizationalUnit]
```

## How It Works

1. **Database returns rows** with filtering columns and a `data` JSONB column
2. **FraiseQL instantiates** types exclusively from the `data` column
3. **Other columns** are ignored during instantiation (they're only for filtering)

## Usage

```python
# Your query uses columns for filtering
query = """
    SELECT * FROM tv_allocation
    WHERE tenant_id = $1
    AND machine_id = $2
"""

# Development mode: Returns typed objects from 'data' column
repo = FraiseQLRepository(pool, {"mode": "development"})
allocation = await repo.find_one("tv_allocation", tenant_id=tenant_id, id=id)
print(allocation.machine.name)  # Fully typed object

# Production mode: Returns raw dict
repo = FraiseQLRepository(pool, {"mode": "production"})
allocation = await repo.find_one("tv_allocation", tenant_id=tenant_id, id=id)
print(allocation["data"]["machine"]["name"])  # Raw dict access
```

## Key Points

- **Single source of truth**: The `data` column contains everything needed for instantiation
- **Clean separation**: Database columns for filtering, JSONB for data
- **No backward compatibility complexity**: FraiseQL expects a `data` column
- **Matches PrintOptim exactly**: Same architecture, same patterns

## Requirements

Your database views/tables MUST have:
- A `data` column (JSONB) containing the complete object representation
- Other columns as needed for filtering, joins, and access control

That's it! Simple and consistent with your existing architecture.
