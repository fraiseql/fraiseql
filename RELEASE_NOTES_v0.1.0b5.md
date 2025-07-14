# Release Notes - FraiseQL v0.1.0b5

**Release Date**: 2025-07-06

## Overview

This release fixes a critical issue with PostgreSQL date serialization in mutations that was preventing proper JSON serialization of date fields. The fix ensures that dates maintain their ISO 8601 format throughout the entire pipeline, from GraphQL input to PostgreSQL processing to JSON response.

## What's Fixed

### PostgreSQL Date Serialization 🗓️

**Problem**: When mutations involved date fields, FraiseQL would fail with "Object of type date is not JSON serializable" errors. This occurred because:
1. GraphQL date scalars converted date strings to Python `date` objects
2. These objects weren't properly serialized when passed to PostgreSQL functions
3. PostgreSQL results containing date columns were converted to Python `date` objects by psycopg3

**Solution**:
- Added proper date serialization in the mutation decorator's `_to_dict` function
- Configured psycopg3 to return date/time columns as ISO 8601 strings instead of Python objects
- Registered custom `TextLoader` for all PostgreSQL date/time types

## Technical Details

### Input Side Fix
```python
# In mutation_decorator.py _to_dict function
elif hasattr(v, "isoformat"):  # date, datetime, time
    result[k] = v.isoformat()
```

### Output Side Fix
```python
# In app.py create_db_pool function
class TextLoader(Loader):
    def load(self, data):
        return data.decode("utf-8") if isinstance(data, bytes) else data

# Register for all date/time types
conn.adapters.register_loader("date", TextLoader)
conn.adapters.register_loader("timestamp", TextLoader)
conn.adapters.register_loader("timestamptz", TextLoader)
```

## Benefits

- ✅ No more JSON serialization errors with date fields
- ✅ Dates maintain ISO 8601 format throughout the pipeline
- ✅ Direct PostgreSQL-to-frontend data flow preserved
- ✅ Compatible with JavaScript date handling
- ✅ No changes required to existing GraphQL schemas or PostgreSQL functions
- ✅ Fully backward compatible

## Example

```python
@fraiseql.input
class CreateEventInput:
    name: str
    event_date: date  # This now works correctly!

@fraiseql.mutation(function="create_event")
class CreateEvent:
    input: CreateEventInput
    success: CreateEventSuccess
    failure: CreateEventError
```

## Who This Affects

This fix is especially important for:
- Applications using date fields in mutations
- Teams migrating from other GraphQL frameworks
- Projects following PostgreSQL best practices for date handling

## Credits

Special thanks to the PrintOptim team for reporting this issue and providing detailed reproduction steps.

## Upgrading

```bash
pip install --upgrade fraiseql==0.1.0b5
```

No code changes are required - the fix is automatic.

## What's Next

We continue to refine FraiseQL's PostgreSQL integration to ensure seamless data flow from database to frontend. Future releases will focus on additional type handling improvements and performance optimizations.
