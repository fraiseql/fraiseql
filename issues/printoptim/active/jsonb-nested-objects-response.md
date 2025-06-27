# Response: JSONB Nested Objects with FraiseQL Development Mode

## Summary

Your approach with `@property` decorators is **not the recommended way** for FraiseQL. The dual-mode feature in v0.1.0a13 already handles JSONB nested object instantiation automatically in development mode.

## The Correct Approach

### 1. Define Your Nested Types

First, ensure all nested types are properly decorated with `@fraise_type`:

```python
from fraiseql import fraise_type, fraise_field
from uuid import UUID
from datetime import date, datetime
from typing import Optional

@fraise_type
class Machine:
    id: UUID
    name: str
    model: str
    serial_number: str
    # ... other fields

@fraise_type
class Location:
    id: UUID
    name: str
    building: str
    floor: str
    room: str
    # ... other fields

@fraise_type
class OrganizationalUnit:
    id: UUID
    name: str
    code: str
    parent_id: Optional[UUID]
    # ... other fields
```

### 2. Define the Allocation Type with Typed JSONB Fields

Here's the key insight: Instead of treating `data` as a generic dict, **define the nested objects as typed fields directly**:

```python
@fraise_type
class Allocation:
    """Allocation type representing machine item allocations."""
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

    # JSONB nested fields - FraiseQL will automatically extract these
    # from the 'data' column in development mode
    identifier: Optional[str]
    machine: Optional[Machine]
    location: Optional[Location]
    organizational_unit: Optional[OrganizationalUnit]
    network_configuration: Optional[dict]  # If this doesn't need to be typed
```

### 3. How FraiseQL Handles This

When FraiseQL's `_instantiate_recursive` method processes your data:

1. It receives the row from the database with the JSONB `data` column
2. It flattens the nested JSONB fields into the top level during instantiation
3. For each field that has a type with `@fraise_type`, it recursively instantiates that type
4. In development mode, you get fully typed nested objects
5. In production mode, you get the raw dict

### 4. The Magic: Database View Structure

The key is how your database view returns the data. Your view should return the JSONB fields at the top level:

```sql
CREATE VIEW tv_allocation AS
SELECT
    a.id,
    a.machine_id,
    a.machine_item_id,
    a.organizational_unit_id,
    a.location_id,
    a.valid_from,
    a.valid_until,
    a.is_past,
    a.is_current,
    a.is_future,
    a.is_reserved,
    a.is_stock,
    -- Extract JSONB fields to top level
    a.data->>'identifier' as identifier,
    a.data->'machine' as machine,
    a.data->'location' as location,
    a.data->'organizational_unit' as organizational_unit,
    a.data->'network_configuration' as network_configuration
FROM allocations a;
```

### 5. Usage in Development Mode

```python
# With FRAISEQL_ENV=development
repo = FraiseQLRepository(pool)
allocation = await repo.find_one("tv_allocation", id=allocation_id)

# All nested objects are automatically instantiated!
print(allocation.machine.name)  # Works - Machine object
print(allocation.location.building)  # Works - Location object
print(allocation.organizational_unit.code)  # Works - OrganizationalUnit object
print(allocation.identifier)  # Works - string

# Type checking works
if isinstance(allocation.machine, Machine):  # True
    process_machine(allocation.machine)
```

### 6. Production Mode Behavior

```python
# With FRAISEQL_ENV=production (default)
allocation = await repo.find_one("tv_allocation", id=allocation_id)

# Returns raw dict - you'd access nested data like:
print(allocation["machine"]["name"])  # Dict access
print(allocation["identifier"])  # Direct field
```

## Why Your @property Approach Won't Work

1. **Properties aren't part of instantiation**: When FraiseQL creates instances, it passes field values to `__init__`. Properties are computed after instantiation.

2. **Type information is lost**: FraiseQL can't know that your property should return a `Machine` instance.

3. **Mode switching breaks**: Your properties would always return dicts, even in development mode.

## Migration Steps

1. **Update your database view** to extract JSONB fields to top level (as shown above)

2. **Update your type definition** to include the nested fields directly (remove the `data` field and `@property` decorators)

3. **Test both modes**:
   ```python
   # Development mode test
   os.environ["FRAISEQL_ENV"] = "development"
   allocation = await repo.find_one("tv_allocation", id=test_id)
   assert isinstance(allocation.machine, Machine)

   # Production mode test
   os.environ["FRAISEQL_ENV"] = "production"
   allocation = await repo.find_one("tv_allocation", id=test_id)
   assert isinstance(allocation["machine"], dict)
   ```

## Alternative: If You Must Keep the JSONB Structure

If you can't modify the database view to extract fields, you'll need to handle instantiation manually:

```python
@fraise_type
class Allocation:
    # ... fields ...
    data: dict

    def get_machine(self) -> Optional[Machine]:
        """Manually instantiate Machine from JSONB data."""
        if not self.data or "machine" not in self.data:
            return None
        return Machine(**self.data["machine"])
```

But this approach:
- Doesn't benefit from FraiseQL's automatic instantiation
- Requires manual work for each nested type
- Won't switch behavior between dev/prod modes

## Recommendation

Modify your database view to extract JSONB fields to the top level. This allows FraiseQL's dual-mode feature to work its magic automatically, giving you the best developer experience in development and optimal performance in production.
