# Field Threshold Tests

This directory contains tests for FraiseQL's field limit threshold functionality.

## Overview

When GraphQL queries request many fields, FraiseQL can optimize the SQL query by returning the full JSONB data column instead of building individual field extractions with `jsonb_build_object()`. This optimization is controlled by the `field_limit_threshold` parameter.

## Test Files

### test_field_limit_threshold.py
Tests the SQL generation logic for field limit thresholds:
- Queries below threshold use `jsonb_build_object()`
- Queries exceeding threshold return full data column
- Edge cases: exact threshold, zero threshold, empty fields
- Nested field counting
- Raw JSON output handling

### test_field_limit_integration.py
Tests the repository integration with field thresholds:
- Repository behavior with and without thresholds
- `find()` and `find_one()` methods
- WHERE clause compatibility
- Type registration and view mapping

### test_graphql_field_limit_e2e.py
End-to-end tests with FastAPI and GraphQL:
- Complete GraphQL queries with many fields
- Product type with 100+ fields for realistic testing
- Query and mutation handling
- Performance validation

## How Field Threshold Works

1. **Below Threshold**: Normal JSONB field extraction
   ```sql
   SELECT jsonb_build_object(
     'id', data->>'id',
     'name', data->>'name',
     'email', data->>'email'
   ) AS result FROM users
   ```

2. **Above Threshold**: Return full data column
   ```sql
   SELECT data FROM users
   ```

## Configuration

Set the threshold when building SQL queries:
```python
query = build_sql_query(
    table="users",
    field_paths=field_paths,
    json_output=True,
    field_limit_threshold=20,  # Switch to full data at 20+ fields
)
```

Or configure in FastAPI:
```python
config = FraiseQLConfig(
    field_limit_threshold=30  # Default threshold for all queries
)
```

## Running Tests

```bash
# Run all field threshold tests
pytest tests/field_threshold/

# Run specific test file
pytest tests/field_threshold/test_field_limit_threshold.py

# Run with database tests
pytest tests/field_threshold/ -m database
```

## Performance Considerations

- Threshold should balance between SQL query size and data transfer
- Typical values: 20-50 fields
- Consider your specific use case and field sizes
- Monitor query performance with different thresholds
