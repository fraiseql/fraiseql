# Corrected Response: Allocation Type with Nested JSONB

## You're Right - Keep the Nested Structure

I apologize for the confusion in my previous response. You're correct - FraiseQL is designed to handle nested JSONB structures automatically. Let me provide the correct approach.

## The Correct Implementation

Your type definition should directly reflect the nested structure:

```python
@fraiseql.type
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

    # The JSONB column containing nested data
    data: Optional["AllocationData"]

@fraiseql.type
class AllocationData:
    """Nested data structure within Allocation."""
    identifier: Optional[str]
    start_date: Optional[date]
    end_date: Optional[date]
    notes: Optional[str]
    notes_contact: Optional[str]
    is_provisionnal: bool = fraiseql.field(default=False)

    # Nested objects
    machine: Optional[Machine]
    location: Optional[Location]
    organizational_unit: Optional[OrganizationalUnit]
    network_configuration: Optional[NetworkConfiguration]
```

## How FraiseQL Handles This

When FraiseQL's `_instantiate_recursive` encounters the `data` field:

1. It sees that `data` is typed as `Optional[AllocationData]`
2. It checks if `AllocationData` has `__fraiseql_definition__` (it does, from `@fraiseql.type`)
3. It recursively instantiates `AllocationData` from the JSONB dict
4. Within `AllocationData`, it recursively instantiates `Machine`, `Location`, etc.

## Keep Your Database View As-Is

Your current view structure is perfect:

```sql
CREATE TABLE public.tv_allocation (
    id uuid NOT NULL,
    machine_id uuid,
    machine_item_id uuid,
    organizational_unit_id uuid,
    location_id uuid,
    valid_from date,
    valid_until date,
    is_past boolean,
    is_current boolean,
    is_future boolean,
    is_reserved boolean,
    is_stock boolean,
    data jsonb,  -- Contains nested objects
    -- ... other fields
);
```

## Usage in Development Mode

```python
# With FRAISEQL_ENV=development
repo = FraiseQLRepository(pool)
allocation = await repo.find_one("tv_allocation", id=allocation_id)

# Nested objects are automatically instantiated!
print(allocation.data.identifier)  # "ALLOC-001"
print(allocation.data.machine.name)  # Fully instantiated Machine
print(allocation.data.location.building)  # Fully instantiated Location

# Type checking works throughout the tree
assert isinstance(allocation, Allocation)
assert isinstance(allocation.data, AllocationData)
assert isinstance(allocation.data.machine, Machine)
```

## Production Mode

```python
# With FRAISEQL_ENV=production (default)
allocation = await repo.find_one("tv_allocation", id=allocation_id)

# Returns raw nested dicts
print(allocation["data"]["identifier"])  # "ALLOC-001"
print(allocation["data"]["machine"]["name"])  # Dict access
```

## Alternative: If You Don't Want a Separate AllocationData Type

You can keep the `data` field as a dict and use properties for convenience:

```python
@fraiseql.type
class Allocation:
    """Allocation type representing machine item allocations."""
    # ... all the direct fields ...

    # Keep as dict - no automatic instantiation of nested objects
    data: dict[str, Any]

    # Convenience properties for common access patterns
    @property
    def identifier(self) -> Optional[str]:
        return self.data.get("identifier") if self.data else None

    @property
    def machine_data(self) -> Optional[dict]:
        """Returns the raw machine data dict."""
        return self.data.get("machine") if self.data else None
```

But this approach loses the automatic instantiation benefits.

## Answers to Your Original Questions

1. **Do you need to modify your database view?** No! Keep it as-is with the nested JSONB structure.

2. **Field mapping:** Create a separate type for the nested structure (`AllocationData`) and FraiseQL handles the rest.

3. **Query usage:** Your current query is perfect and compatible.

4. **Duplicate date fields:** These serve different purposes - `valid_from/until` are allocation validity, `start_date/end_date` might be contract dates. Keep both.

## Summary

The key insight: FraiseQL's `_instantiate_recursive` method is designed to handle nested structures. By creating a type for your nested data (`AllocationData`), you enable FraiseQL to traverse and instantiate the entire object graph automatically in development mode.

I apologize for the confusion in my previous response - FraiseQL is indeed smart enough to handle nested JSONB structures without flattening them in the database view.
