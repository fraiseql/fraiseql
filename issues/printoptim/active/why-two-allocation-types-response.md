# Response: Why Two Types for Allocation

## You're Absolutely Right - It's Not Ideal

Your observation is spot-on. Having two types for one conceptual entity (Allocation) is indeed awkward. Let me explain why this is currently necessary and provide some alternatives.

## Why Two Types Are Currently Needed

FraiseQL's `_instantiate_recursive` method works with the exact structure returned by your database query. When your query returns:

```json
{
  "id": "123",
  "machine_id": "456",
  "is_current": true,
  "data": {
    "identifier": "ALLOC-001",
    "machine": {...},
    "location": {...}
  }
}
```

FraiseQL maps fields directly - it doesn't "reach into" the `data` field to extract nested properties to the top level. This is why you need:
- `Allocation` type matching the outer structure
- `AllocationData` type matching the nested JSONB structure

## Alternative Solutions

### Option 1: Modify Your Database View (Recommended)

The cleanest solution is to flatten the JSONB fields in your database view:

```sql
CREATE OR REPLACE VIEW app.tv_allocation_flat AS
SELECT
    -- Direct columns
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

    -- Flatten JSONB fields to top level
    a.data->>'identifier' as identifier,
    (a.data->>'start_date')::date as start_date,
    (a.data->>'end_date')::date as end_date,
    a.data->>'notes' as notes,
    a.data->>'notes_contact' as notes_contact,
    (a.data->>'is_provisionnal')::boolean as is_provisionnal,
    a.data->'machine' as machine,  -- Keep as JSONB for nested object
    a.data->'location' as location,  -- Keep as JSONB for nested object
    a.data->'organizational_unit' as organizational_unit,
    a.data->'network_configuration' as network_configuration
FROM public.tv_allocation a;
```

Then you can have a single type:

```python
@fraise_type
class Allocation:
    """Single allocation type with all fields."""
    # Direct columns
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

    # From JSONB (now flattened)
    identifier: Optional[str]
    start_date: Optional[date]
    end_date: Optional[date]
    notes: Optional[str]
    notes_contact: Optional[str]
    is_provisionnal: bool = fraise_field(default=False)

    # Nested objects still work!
    machine: Optional[Machine]
    location: Optional[Location]
    organizational_unit: Optional[OrganizationalUnit]
    network_configuration: Optional[NetworkConfiguration]
```

### Option 2: Use Properties for Convenience (Keep Two Types)

If you can't modify the view, you can make the two-type pattern more ergonomic:

```python
@fraise_type
class Allocation:
    """Allocation with convenience properties."""
    id: UUID
    machine_id: Optional[UUID]
    # ... other direct fields ...
    data: Optional[AllocationData]

    # Convenience properties to access nested data
    @property
    def identifier(self) -> Optional[str]:
        return self.data.identifier if self.data else None

    @property
    def machine(self) -> Optional[Machine]:
        return self.data.machine if self.data else None

    @property
    def location(self) -> Optional[Location]:
        return self.data.location if self.data else None

    # ... more properties for frequently accessed fields
```

This gives you `allocation.machine` instead of `allocation.data.machine`.

### Option 3: Post-Processing After Query

Create a "merged" type and manually combine after querying:

```python
@fraise_type
class MergedAllocation:
    """Single type combining both structures."""
    # All fields in one place
    id: UUID
    machine_id: Optional[UUID]
    identifier: Optional[str]
    machine: Optional[Machine]
    # ... all other fields

async def get_allocation(repo, allocation_id):
    # Get the nested structure
    result = await repo.find_one("tv_allocation", id=allocation_id)

    if repo.mode == "development":
        # Manually merge in development mode
        merged_data = {
            **{k: v for k, v in result.__dict__.items() if k != 'data'},
            **(result.data.__dict__ if result.data else {})
        }
        return MergedAllocation(**merged_data)
    else:
        # In production, manually merge dicts
        merged_data = {
            **{k: v for k, v in result.items() if k != 'data'},
            **(result.get('data', {}) or {})
        }
        return merged_data
```

## Recommendation

I recommend **Option 1** (modifying the database view) because:
- Single, clean type definition
- No mental model mismatch
- FraiseQL's automatic instantiation works perfectly
- No additional code complexity

The view modification is a one-time database change that makes your Python code much cleaner.

## Future FraiseQL Enhancement

This is valuable feedback for FraiseQL. A future enhancement could be a field annotation that tells FraiseQL to extract nested fields:

```python
# Hypothetical future syntax
@fraise_type
class Allocation:
    id: UUID
    machine_id: Optional[UUID]

    # Tell FraiseQL these come from data JSONB column
    identifier: Optional[str] = fraise_field(source="data.identifier")
    machine: Optional[Machine] = fraise_field(source="data.machine")
```

But this doesn't exist yet, so one of the above options is needed for now.
