# GraphQL Type Registration Error: Duplicate Date Type Definition

**Status**: ✅ Resolved
**Priority**: High - Prevents printoptim_backend from upgrading to latest FraiseQL

## Summary
FraiseQL encounters a GraphQL type registration error when registering the Date scalar type, with GraphQL reporting that a type named "Date" has already been defined. This prevents applications that use the Date scalar from starting up properly.

## Current Behavior
When attempting to use the Date scalar type in a FraiseQL application, the following error occurs:
```
graphql.error.graphql_error.GraphQLError: Schema must contain uniquely named types but contains multiple types named "Date"
```

## Expected Behavior
The Date scalar type should be registered once and be available for use in GraphQL schemas without causing duplicate type errors.

## Impact
1. **Application Startup Failure**: Applications using Date fields cannot start
2. **Migration Blocker**: Projects cannot upgrade to newer FraiseQL versions
3. **Type System Integrity**: Suggests underlying issues with type registration lifecycle

## Root Cause Analysis
Based on code inspection, the issue appears to stem from:

1. **Multiple Registration Points**: The `DateScalar` is defined in `/src/fraiseql/types/scalars/date.py` and mapped in `graphql_utils.py`
2. **Type Cache Management**: The `_graphql_type_cache` in `graphql_type.py` may not be preventing duplicate registrations
3. **Schema Building Process**: The schema builder may be processing scalar types multiple times

## Reproduction Steps
1. Create a FraiseQL type with a Date field:
```python
import datetime
from fraiseql import fraise_type

@fraise_type
class Event:
    name: str
    event_date: datetime.date
```

2. Build the schema and start the application
3. Observe the GraphQL error about duplicate "Date" type

## Suggested Solutions

### Solution 1: Singleton Pattern for Scalar Registration
Ensure scalar types are only registered once by implementing a registry check:
```python
# In schema_builder.py or graphql_type.py
_registered_scalars: set[str] = set()

def register_scalar_once(scalar: GraphQLScalarType) -> GraphQLScalarType:
    if scalar.name in _registered_scalars:
        return get_existing_scalar(scalar.name)
    _registered_scalars.add(scalar.name)
    return scalar
```

### Solution 2: Fix Type Cache Key Generation
The cache key in `_graphql_type_cache` might not be unique enough for scalars:
```python
# Current: key = (annotation.name, typ.__module__)
# Proposed: Include type category in key
key = (f"scalar_{scalar.name}", "fraiseql.scalars")
```

### Solution 3: Lazy Scalar Registration
Only register scalars when they're actually used in the schema:
```python
def convert_scalar_to_graphql(typ: type) -> GraphQLScalarType:
    # Return a reference that's resolved during schema building
    return get_or_create_scalar(typ)
```

## Related Code Locations
- `/src/fraiseql/types/scalars/date.py` - Date scalar definition
- `/src/fraiseql/types/scalars/graphql_utils.py` - Scalar type mapping
- `/src/fraiseql/core/graphql_type.py` - Type conversion and caching
- `/src/fraiseql/gql/schema_builder.py` - Schema building and type registration

## Environment
- FraiseQL version: Latest (post-0.1.0a7)
- Python version: 3.11+
- GraphQL-core version: (as per pyproject.toml dependencies)
- Use case: printoptim_backend attempting to upgrade FraiseQL

## Additional Context
This issue is blocking the printoptim_backend project from upgrading to the latest FraiseQL version. The project uses Date fields extensively in its models, making this a critical blocker for adoption of newer FraiseQL features.

## Test Case
A test should be added to verify scalar types are only registered once:
```python
def test_date_scalar_single_registration():
    """Ensure Date scalar is only registered once in the schema."""
    from fraiseql import build_fraiseql_schema

    @fraise_type
    class Model1:
        date1: datetime.date

    @fraise_type
    class Model2:
        date2: datetime.date

    # This should not raise a duplicate type error
    schema = build_fraiseql_schema(types=[Model1, Model2])
    assert "Date" in schema.type_map
```

## Resolution

**Fixed in**: June 19, 2025
**Solution implemented**: Scalar Type Caching in GraphQL Type Conversion

### Changes Made

1. **Added scalar type caching** in `src/fraiseql/core/graphql_type.py`:
   - Scalar types are now cached using the pattern `(f"scalar_{typ.__name__}", typ.__module__)`
   - Cache is checked before creating new scalar instances
   - Prevents duplicate scalar type registrations

2. **Added comprehensive tests** in `tests/types/scalars/`:
   - `test_date_registration.py` - Basic Date scalar registration tests
   - `test_scalar_caching_fix.py` - Comprehensive caching validation
   - Tests cover single registration, multiple schema builds, and complex scenarios

### Technical Details

The root cause was that scalar types (like `DateScalar`) were not being cached in the `_graphql_type_cache`, unlike user-defined types. This could potentially lead to multiple instances of the same scalar being created during schema building, causing GraphQL's uniqueness validation to fail.

The fix ensures that:
- Scalar types are cached on first conversion
- Subsequent requests for the same scalar return the cached instance
- Cache keys use a `scalar_` prefix to avoid conflicts with regular types
- All built-in scalars (Date, DateTime, UUID, etc.) benefit from this caching

### Verification

All existing tests pass, and new tests verify:
- ✅ Date scalars are cached correctly
- ✅ Multiple Date fields reference the same scalar instance
- ✅ Complex schemas with many Date fields work properly
- ✅ Multiple schema builds don't cause registration conflicts

---
*Reported by: printoptim_backend development team*
*Date: June 19, 2025*
*Resolved by: Claude Code*
*Resolution date: June 19, 2025*
