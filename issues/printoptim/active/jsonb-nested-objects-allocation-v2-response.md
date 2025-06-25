# Response: Allocation Type Implementation Review

## Your Implementation Needs Adjustment

Your proposed implementation is on the right track, but **you need to modify your database view** to make it work with FraiseQL's automatic instantiation.

## The Issue

FraiseQL's `_instantiate_recursive` method expects fields to be at the top level of the row dictionary. It doesn't automatically extract nested fields from a `data` JSONB column. When it receives a row like:

```python
{
    "id": "...",
    "machine_id": "...",
    "data": {"identifier": "ALLOC-001", "machine": {...}}
}
```

It will only see `id`, `machine_id`, and `data` as fields to map to your type.

## Required Database View Modification

You need to modify your view to extract JSONB fields to the top level:

```sql
CREATE OR REPLACE VIEW app.tv_allocation AS
SELECT 
    -- Direct table columns
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
    
    -- Extract scalar fields from JSONB
    a.data->>'identifier' as identifier,
    (a.data->>'start_date')::date as start_date,
    (a.data->>'end_date')::date as end_date,
    a.data->>'notes' as notes,
    a.data->>'notes_contact' as notes_contact,
    (a.data->>'is_provisionnal')::boolean as is_provisionnal,
    
    -- Extract nested objects as JSONB (not text)
    a.data->'machine' as machine,
    a.data->'location' as location,
    a.data->'organizational_unit' as organizational_unit,
    a.data->'network_configuration' as network_configuration,
    
    -- Keep the original data column if needed for other purposes
    a.data as data
FROM public.tv_allocation a;
```

**Important distinctions:**
- Use `->>` for scalar fields (returns text)
- Use `->` for nested objects (returns JSONB)
- Cast scalars to appropriate types (::date, ::boolean)

## Corrected Type Definition

Your type definition is correct! Just ensure the default value syntax:

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
    
    # Fields extracted from JSONB
    identifier: Optional[str]
    start_date: Optional[date]
    end_date: Optional[date]
    notes: Optional[str]
    notes_contact: Optional[str]
    is_provisionnal: bool = fraiseql.field(default=False)  # Use field() for defaults
    
    # Nested objects - FraiseQL will instantiate these
    machine: Optional[Machine]
    location: Optional[Location]
    organizational_unit: Optional[OrganizationalUnit]
    network_configuration: Optional[NetworkConfiguration]
```

## Answers to Your Questions

### 1. Do you need to modify your database view?
**Yes**, you must modify the view to extract JSONB fields to the top level. FraiseQL doesn't automatically traverse into a `data` column.

### 2. Field mapping
With the modified view, FraiseQL will map fields directly. The field `identifier` in your type will map to the `identifier` column from your view (which extracts from `data->>'identifier'`).

### 3. Query usage
You'll need to update your query to use the new view structure:

```python
# Using FraiseQL's find methods (recommended)
allocations = await repo.find("tv_allocation", tenant_id=tenant_id)

# Or if using raw SQL, it works with the modified view
query = """
    SELECT * FROM app.tv_allocation
    WHERE tenant_id = $1
"""
```

### 4. Handling duplicate date fields
You have two options:

**Option A: Use different field names (recommended)**
```python
@fraiseql.type
class Allocation:
    # Database columns
    valid_from: date      # From table column
    valid_until: Optional[date]  # From table column
    
    # JSONB fields (different names)
    start_date: Optional[date]   # From data->>'start_date'
    end_date: Optional[date]     # From data->>'end_date'
```

**Option B: Choose one source of truth**
If `start_date`/`end_date` in JSONB are the same as `valid_from`/`valid_until`, modify your view to use only one:

```sql
-- Use COALESCE to prefer JSONB data if present
COALESCE((a.data->>'start_date')::date, a.valid_from) as valid_from,
COALESCE((a.data->>'end_date')::date, a.valid_until) as valid_until,
```

## Complete Working Example

Here's how it all comes together:

```python
# Enable development mode
os.environ["FRAISEQL_ENV"] = "development"

# Query using FraiseQL
repo = FraiseQLRepository(pool)
allocation = await repo.find_one("tv_allocation", id=allocation_id)

# All nested objects are instantiated!
print(allocation.identifier)  # "ALLOC-001"
print(allocation.machine.name)  # Fully instantiated Machine object
print(allocation.location.building)  # Fully instantiated Location object

# Type checking works
assert isinstance(allocation, Allocation)
assert isinstance(allocation.machine, Machine)
```

## Migration Steps

1. **Create the modified view** (as shown above)
2. **Test the view** returns data correctly
3. **Update your repository code** to use `find()`/`find_one()`
4. **Test in both modes** (dev and production)

The key insight: FraiseQL needs the fields at the top level of the query result to map them to your type attributes.