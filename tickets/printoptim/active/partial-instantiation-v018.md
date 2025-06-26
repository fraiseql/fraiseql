# Partial Object Instantiation Fixed in FraiseQL v0.1.0a18

## Issue Resolution

The nested object instantiation issue has been fixed in v0.1.0a18. FraiseQL now supports partial object instantiation in development mode, allowing GraphQL queries to request only the fields they need from nested objects.

## What Was Fixed

Previously, when querying nested objects, FraiseQL would try to instantiate the complete object with all required fields, even if the GraphQL query only requested a subset:

```graphql
query {
  allocations {
    id
    machine {
      id
      identifier  # Only 2 fields requested, but all were required
    }
  }
}
```

This would fail with errors like "missing required argument: 'machine_serial_number'".

## How It Works Now

In v0.1.0a18, FraiseQL uses partial object instantiation in development mode:

1. **Partial Instantiation**: Objects are created with only the requested fields
2. **Missing Fields**: Required fields that weren't requested are set to `None`
3. **Nested Objects**: Works recursively for all nested objects
4. **Development Only**: This behavior is active in development mode only

## Migration Guide

Simply upgrade to v0.1.0a18:

```bash
pip install fraiseql==0.1.0a18
```

No code changes are required. Your existing queries will now work correctly.

## Example

This query now works without errors:

```graphql
query GetAllocations {
  allocations {
    id
    identifier
    machine {
      id
      identifier
    }
  }
}
```

Even though `Machine` has many required fields like `machine_serial_number` and `model`, the query succeeds because only `id` and `identifier` are instantiated.

## Technical Details

- Partial instances are marked with `__fraiseql_partial__` attribute
- Available fields are tracked in `__fraiseql_fields__` attribute
- Works with both dataclasses and regular classes
- Handles `__post_init__` methods that might validate fields

## Important Notes

1. **Type Safety**: While objects are instantiated, missing fields will be `None` even if they're required in the type definition
2. **Production Mode**: In production mode, raw dictionaries are returned (no instantiation)
3. **Full Objects**: If you need all fields, request them in your GraphQL query

## Verification

You can verify partial instantiation is working:

```python
from fraiseql.partial_instantiation import is_partial_instance

# In a resolver
result = await db.find("tv_allocation")
if result and is_partial_instance(result[0].machine):
    print("Machine is a partial instance")
```

## Summary

This fix restores the GraphQL principle of requesting only needed data while maintaining type safety in development mode. Nested queries now work as expected without requiring all fields to be present.