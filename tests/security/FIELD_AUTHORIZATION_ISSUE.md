# Field Authorization Decorator Issue

## Problem Description
The `@authorize_field` decorator has compatibility issues when used with the `@field` decorator, resulting in coroutine wrapping errors.

## Error Pattern
```
RuntimeWarning: coroutine 'wrap_resolver_with_enum_serialization.<locals>.wrapped_resolver' was never awaited
GraphQLError('String cannot represent value: <coroutine wrapped_resolver>'
```

## Root Cause
The issue occurs due to multiple layers of wrapping:
1. `@field` decorator wraps the method to handle GraphQL field resolution
2. `@authorize_field` wraps the resolver to add authorization checks
3. `wrap_resolver_with_enum_serialization` wraps again for enum handling
4. Somewhere in this chain, async/sync detection fails, causing a coroutine to be returned instead of the actual value

## Attempted Solutions
1. Tried different decorator ordering (`@field` then `@authorize_field` vs reverse)
2. Simplified test cases to isolate the issue
3. Used proper FraiseQL type definitions with attributes instead of private fields

## Current Workaround
For now, field-level authorization can be implemented within the field method itself:

```python
@field
def sensitive_field(self, info) -> str:
    if not info.context.get("is_authorized", False):
        raise FieldAuthorizationError("Not authorized")
    return self._sensitive_data
```

## Required Framework Changes
To properly fix this issue, the framework needs:
1. Better async/sync detection in decorator composition
2. Ensure `wrap_resolver_with_enum_serialization` correctly handles already-wrapped resolvers
3. Proper preservation of function signatures through multiple decorator layers
4. Clear documentation on the correct order for applying decorators

## Test Files Affected
- `tests/security/test_field_authorization.py` - Original tests that fail
- `tests/security/test_field_authorization_fixed.py` - Attempted fixes that still fail
- `tests/security/test_field_authorization_simple.py` - Simplified version that works without decorators

## References
- Issue occurs in `src/fraiseql/gql/enum_serializer.py` at `wrap_resolver_with_enum_serialization`
- Decorator implementation in `src/fraiseql/security/field_auth.py`
- Field decorator in `src/fraiseql/decorators.py`
