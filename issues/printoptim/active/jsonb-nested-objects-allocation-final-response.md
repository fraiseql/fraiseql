# Response: JSONB Nested Objects Implementation for Allocation

## Your Approach Needs a Small but Important Change

Your implementation is very close! The key is to create a typed structure for your JSONB data instead of using `dict`.

## The Correct Implementation

```python
from uuid import UUID
from datetime import date
from typing import Optional
from fraiseql import fraise_type, fraise_field

@fraise_type
class AllocationData:
    """The nested data structure stored in the JSONB column."""
    identifier: Optional[str]
    start_date: Optional[date]
    end_date: Optional[date]
    notes: Optional[str]
    notes_contact: Optional[str]
    is_provisionnal: bool = fraise_field(default=False)

    # Nested objects - FraiseQL will recursively instantiate these
    machine: Optional[Machine]
    location: Optional[Location]
    organizational_unit: Optional[OrganizationalUnit]
    network_configuration: Optional[NetworkConfiguration]

@fraise_type
class Allocation:
    """Allocation type representing machine item allocations."""
    # Direct fields from table columns
    id: UUID
    machine_id: Optional[UUID]
    machine_item_id: Optional[UUID]
    organizational_unit_id: Optional[UUID]
    location_id: Optional[UUID]
    valid_from: date
    valid_until: Optional[date]
    is_past: bool
    is_current: bool
    is_future: bool
    is_reserved: bool
    is_stock: bool

    # The JSONB column - typed as AllocationData, not dict!
    data: Optional[AllocationData]
```

## How This Works

When FraiseQL processes your query results in development mode:

1. It sees `data` is typed as `Optional[AllocationData]`
2. Since `AllocationData` is decorated with `@fraise_type`, it has the special `__fraiseql_definition__` attribute
3. FraiseQL's `_instantiate_recursive` method automatically converts the JSONB dict into an `AllocationData` instance
4. Within `AllocationData`, it recursively instantiates all nested objects (`Machine`, `Location`, etc.)

## Your Database Structure Stays the Same

Keep your view exactly as it is:
```sql
SELECT
    id, machine_id, organizational_unit_id, location_id,
    valid_from, valid_until, is_past, is_current, is_future,
    is_reserved, is_stock,
    data  -- JSONB column with nested structure
FROM app.tv_allocation
```

## Usage Examples

### Development Mode
```python
# FRAISEQL_ENV=development
repo = FraiseQLRepository(pool)
allocation = await repo.find_one("tv_allocation", id=allocation_id)

# Everything is fully instantiated!
print(allocation.data.identifier)  # "ALLOC-001"
print(allocation.data.machine.name)  # Machine instance
print(allocation.data.location.building)  # Location instance
print(isinstance(allocation.data.machine, Machine))  # True

# Direct fields work as expected
print(allocation.valid_from)  # date object
print(allocation.machine_id)  # UUID object
```

### Production Mode
```python
# FRAISEQL_ENV=production (default)
allocation = await repo.find_one("tv_allocation", id=allocation_id)

# Raw dict access for maximum performance
print(allocation["data"]["identifier"])
print(allocation["data"]["machine"]["name"])
```

## Answers to Your Specific Questions

1. **Do you need to modify your database view?**
   No! Your current structure with the `data` JSONB column is perfect.

2. **Field mapping?**
   Fields are accessed through the nested structure: `allocation.data.identifier` instead of trying to flatten them.

3. **Query usage?**
   Your current query is compatible. FraiseQL handles the JSONB → AllocationData conversion.

4. **Duplicate date fields?**
   Keep both - they likely serve different purposes:
   - `valid_from/valid_until`: When the allocation is valid
   - `data.start_date/data.end_date`: Contract or agreement dates

## Why Not Use @property?

Your original `@property` approach won't work because:
- Properties are computed after instantiation
- FraiseQL can't detect that properties should return typed objects
- You'd lose the automatic dev/prod mode switching

## Migration Steps

1. Create the `AllocationData` type with `@fraise_type`
2. Change `data: dict` to `data: Optional[AllocationData]` in your Allocation type
3. Import and ensure all nested types (`Machine`, `Location`, etc.) are properly decorated with `@fraise_type`
4. Test in both development and production modes

That's it! FraiseQL will handle all the complex instantiation logic for you.
