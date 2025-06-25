# JSONB Column with Nested Objects - Allocation Type

## Context

In PrintOptim, we have an `Allocation` type that includes a `data` JSONB column containing nested objects. The database view `tv_allocation` has this structure:

```sql
-- From tv_allocation table
data jsonb -- Contains nested JSON like:
-- {
--   "id": "...",
--   "identifier": "...",
--   "start_date": "...",
--   "end_date": "...",
--   "machine": { /* full machine object */ },
--   "location": { /* full location object */ },
--   "organizational_unit": { /* full org unit object */ },
--   "network_configuration": { /* full network config object */ }
-- }
```

## Current Implementation

```python
@fraiseql.type
class Allocation:
    """Allocation type representing machine item allocations."""
    id: uuid.UUID
    machine_id: uuid.UUID | None
    machine_item_id: uuid.UUID | None
    organizational_unit_id: uuid.UUID | None
    location_id: uuid.UUID | None
    valid_from: date
    valid_until: date | None
    is_past: bool
    is_current: bool
    is_future: bool
    is_reserved: bool
    is_stock: bool
    data: dict
```

## Our Attempted Solution

Based on the development mode documentation, we tried using Python properties:

```python
@fraiseql.type
class Allocation:
    """Allocation type representing machine item allocations."""
    id: uuid.UUID
    machine_id: uuid.UUID | None
    machine_item_id: uuid.UUID | None
    organizational_unit_id: uuid.UUID | None
    location_id: uuid.UUID | None
    valid_from: date
    valid_until: date | None
    is_past: bool
    is_current: bool
    is_future: bool
    is_reserved: bool
    is_stock: bool
    data: dict[str, Any]
    
    # Properties to access nested data from the JSONB column
    @property
    def identifier(self) -> str | None:
        """Human-readable allocation identifier."""
        return self.data.get("identifier") if self.data else None
    
    @property
    def machine(self) -> "Machine | None":
        """The allocated machine details from the nested JSONB data."""
        if not self.data:
            return None
        machine_data = self.data.get("machine")
        if not machine_data:
            return None
        # In development mode, FraiseQL should handle instantiation?
        # In production mode, this returns the dict?
        return machine_data
    
    @property
    def location(self) -> "Location | None":
        """The allocation location details from the nested JSONB data."""
        if not self.data:
            return None
        location_data = self.data.get("location")
        return location_data
    
    # ... similar properties for organizational_unit, network_configuration, etc.
```

## Questions

1. Is this the correct approach for exposing nested objects from a JSONB column?
2. Should we be using `@fraiseql.field` instead of `@property`?
3. Does FraiseQL automatically instantiate the nested types (Machine, Location, etc.) from the JSONB data in development mode?
4. Is there a simpler/more elegant way to handle this pattern?

The comment in the migration mentioned "fraiseql automatically instantiates the types if there is a jsonb column that returns the data with all the imbricated objects" - but we're not sure if our implementation is correct.

What's the recommended FraiseQL approach for this use case?