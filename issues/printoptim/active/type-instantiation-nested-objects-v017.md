# FraiseQL v0.1.0a17 - Type Instantiation Issue with Nested Objects

## Issue Description

In FraiseQL v0.1.0a17, there's a critical issue with type instantiation when nested objects are queried. The framework attempts to instantiate nested types with all required fields, even when only a subset of fields is requested in the GraphQL query.

## Environment

- FraiseQL version: 0.1.0a17
- Python version: 3.13
- Database: PostgreSQL with JSONB columns
- Pattern: CQRS with tv_* tables containing JSONB data

## Steps to Reproduce

1. Define types with nested objects:

```python
@fraiseql.type
class Machine:
    id: uuid.UUID
    identifier: str
    machine_serial_number: str
    # ... many other required fields
    model: Model  # Nested object

@fraiseql.type
class Allocation:
    id: uuid.UUID
    identifier: str
    start_date: date
    machine: Machine  # Nested Machine object
```

2. Create a query that requests only partial fields from nested objects:

```graphql
query GetAllocations {
  allocations {
    id
    identifier
    machine {
      id
      identifier  # Only requesting 2 fields, not all required fields
    }
  }
}
```

3. Execute the query

## Expected Behavior

FraiseQL should instantiate the nested `Machine` object with only the requested fields (`id` and `identifier`), ignoring other required fields that weren't requested.

## Actual Behavior

FraiseQL attempts to instantiate the full `Machine` object with all required fields, resulting in errors:

```json
{
  "errors": [{
    "message": "missing a required argument: 'machine_serial_number'"
  }]
}
```

Or in some cases:

```json
{
  "errors": [{
    "message": "missing a required argument: 'model'"
  }]
}
```

## Database Structure

The data is stored in a JSONB column with complete nested objects:

```sql
-- tv_allocation table
id: uuid
tenant_id: uuid  
identifier: text
data: jsonb  -- Contains full nested objects

-- Example data column:
{
  "id": "650e8400-e29b-41d4-a716-446655440001",
  "identifier": "ALLOC-001",
  "machine": {
    "id": "1451ff31-5511-0000-0000-000000000001",
    "identifier": "MACHINE-001",
    "machine_serial_number": "SN-001-2024",
    "model": { ... },  // Full model data
    // ... all other machine fields
  }
}
```

## Impact

This issue prevents querying any type that contains nested objects, unless the query requests ALL required fields from the nested types. This is a significant limitation as it:

1. Forces over-fetching of data
2. Breaks the GraphQL principle of requesting only needed fields
3. Makes it impossible to have simple queries on complex nested structures

## Workaround Attempts

1. Tried making all fields optional - This works but loses type safety
2. Tried using `@property` decorators - Team advised this was wrong approach
3. Current workaround: Only top-level queries work (e.g., `machines` query works, but `allocations` with nested `machine` fails)

## Code Context

Repository structure:
- Types defined in: `src/printoptim_backend/entrypoints/api/gql_types/`
- Queries in: `src/printoptim_backend/entrypoints/api/resolvers/queries.py`
- View registration in: `src/printoptim_backend/entrypoints/api/register_views.py`

## Suggested Fix

FraiseQL should only validate and instantiate fields that are actually requested in the GraphQL query selection set, not all fields defined in the type. This would align with standard GraphQL behavior where partial object queries are the norm.

## Additional Notes

- Machine queries work fine when queried directly
- The issue only occurs with nested objects
- The JSONB data contains all required fields, so it's purely a type instantiation issue
- This worked differently in earlier versions where we could control instantiation more directly