# FraiseQL Update: JSONB Data Column Pattern Support

## Good News!

Based on your feedback, FraiseQL v0.1.0a13 has been updated to use the same architecture as PrintOptim. Object instantiation now comes exclusively from a JSONB `data` column.

## What Changed

FraiseQL now expects your tables/views to follow this pattern:

```sql
-- Example: tv_allocation
SELECT
    id,              -- For primary key
    tenant_id,       -- For access control
    machine_id,      -- For filtering/joins
    location_id,     -- For filtering/joins
    data             -- JSONB column with complete object (REQUIRED)
FROM allocations
WHERE ...
```

## Your Type Definition

You only need ONE type that matches the structure in the `data` column:

```python
@fraise_type
class Allocation:
    """All fields come from the JSONB data column."""
    id: UUID
    identifier: str
    machine_id: Optional[UUID]
    location_id: Optional[UUID]
    valid_from: date
    valid_until: Optional[date]
    is_current: bool
    notes: Optional[str]

    # Nested objects are automatically instantiated
    machine: Optional[Machine]
    location: Optional[Location]
    organizational_unit: Optional[OrganizationalUnit]
```

## How Development Mode Works

```python
# FraiseQL instantiates ONLY from the 'data' column
allocation = await repo.find_one("tv_allocation", id=allocation_id)

# You get fully typed objects
print(allocation.identifier)        # "ALLOC-001"
print(allocation.machine.name)      # Machine instance
print(allocation.location.building) # Location instance
```

## Important Notes

1. **No more dual types** - You don't need separate `Allocation` and `AllocationData` types
2. **`data` column is required** - FraiseQL will raise an error if it's missing
3. **Other columns are ignored** - They're only used for WHERE clauses, not instantiation
4. **Simple and consistent** - Matches your existing PrintOptim architecture exactly

## Migration

No changes needed to your database! Your existing views with `data` JSONB columns work perfectly. Just:

1. Update to FraiseQL v0.1.0a13
2. Define your types to match the `data` column structure
3. Remove any workarounds like `@property` methods

## Summary

- FraiseQL now uses the same pattern as PrintOptim
- Types are instantiated exclusively from the `data` JSONB column
- Other columns are for filtering and access control only
- Single type definition per entity
- No complex nested structures needed

This should resolve all the issues you raised about the two-type pattern and make FraiseQL work exactly like your existing system!
