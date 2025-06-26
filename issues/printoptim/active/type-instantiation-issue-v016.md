# Type Instantiation Issue in FraiseQL v0.1.0a16

## Issue Description

After upgrading to FraiseQL v0.1.0a16, queries are returning raw database rows instead of properly instantiated GraphQL types. The data is being fetched correctly from the database, but FraiseQL is failing to convert the rows to the expected types.

## Environment

- FraiseQL version: 0.1.0a16
- Python version: 3.13
- Environment mode: development
- Database: PostgreSQL with JSONB data columns

## Current Setup

### Configuration
```python
fraiseql_config = FraiseQLConfig(
    _env_file=None,
    database_url=settings.database_url,
    environment="development",  # Explicitly set to development
    enable_introspection=True,
    enable_playground=True,
    playground_tool="apollo-sandbox",
    auth_enabled=False,
)
```

### View Registration
```python
from fraiseql.db import register_type_for_view

def register_views():
    """Register view-to-type mappings for FraiseQL."""
    register_type_for_view("tv_machine", Machine)
    register_type_for_view("tv_allocation", Allocation)

# Called during app initialization
register_views()
```

### Database Views
```sql
CREATE VIEW tv_allocation AS
SELECT 
    id,
    tenant_id,
    identifier,
    start_date,
    end_date,
    jsonb_build_object(
        'id', id,
        'identifier', identifier,
        'start_date', start_date,
        'end_date', end_date,
        'notes', 'Sample note',
        'notes_contact', 'Contact info',
        'is_provisionnal', false,
        'machine', jsonb_build_object(...),
        'location', jsonb_build_object(...),
        'organizational_unit', jsonb_build_object(...),
        'network_configuration', jsonb_build_object(...)
    ) AS data
FROM ...
```

## Error Details

When querying:
```graphql
query {
  allocations(limit: 3) {
    id
    identifier
    startDate
    endDate
  }
}
```

Response:
```json
{
  "data": {
    "allocations": [null, null, null]
  },
  "errors": [
    {
      "message": "Expected value of type 'Allocation' but got: {'id': <UUID instance>, 'tenant_id': <UUID instance>, 'identifier': 'ALLOC-001', 'start_date': <date instance>, 'end_date': <date instance>, 'data': {...}}.",
      "locations": [{"line": 1, "column": 27}],
      "path": ["allocations", 0]
    }
    // ... similar errors for other items
  ]
}
```

## What's Happening

1. The database query is executing successfully
2. The rows are being fetched with the correct structure (id, tenant_id, identifier, start_date, end_date, data)
3. FraiseQL is not instantiating the Allocation type from the row data
4. Instead, it's returning the raw dict/row and complaining it's not an Allocation instance

## Expected Behavior

In development mode with registered views, FraiseQL should:
1. Fetch the rows from the database
2. Use the JSONB `data` column to instantiate the registered type (Allocation)
3. Return properly typed GraphQL objects

## Questions

1. Has the type instantiation mechanism changed in v0.1.0a16?
2. Is there a new way to register view-to-type mappings?
3. Are we missing a configuration setting for development mode?
4. Is the `register_type_for_view` function still the correct approach?

## Additional Context

- This was working in earlier versions (the pattern matches the documentation)
- The debug context shows we're in development mode
- The JSONB data structure matches the GraphQL type fields
- Both snake_case and camelCase field names have been tried in the JSONB

## Temporary Workaround Needed

Please advise on how to get type instantiation working in v0.1.0a16 so we can continue development and provide sample queries with results.