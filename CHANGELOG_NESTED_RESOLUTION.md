# Nested Object Resolution Feature

## Summary

Added a new `resolve_nested` parameter to the `@type` decorator that controls how nested objects with `sql_source` are resolved. This feature solves the common issue where FraiseQL would incorrectly attempt to query nested objects separately even when their data was already embedded in the parent's JSONB column.

## New Feature: `resolve_nested` Parameter

### Syntax
```python
@type(
    sql_source="table_name",
    resolve_nested=False  # Default: assumes embedded data
)
```

### Default Behavior (resolve_nested=False)
- Assumes nested objects are embedded in parent's JSONB data
- No separate queries made for nested objects
- No tenant_id required for nested fields
- Better performance (avoids N+1 queries)
- Works well with PostgreSQL views that pre-join data

### Explicit Nested Resolution (resolve_nested=True)
- Makes separate queries to nested type's sql_source
- May require tenant_id and other context parameters
- Useful for true relational data not embedded in parent
- Allows fresh data from source tables

## Changes Made

### 1. Core Type System
- **`src/fraiseql/types/fraise_type.py`**: Added `resolve_nested` parameter with documentation and examples
- **`src/fraiseql/types/definitions.py`**: Added `resolve_nested` to `FraiseQLTypeDefinition` class
- **`src/fraiseql/types/constructor.py`**: Updated type constructor to pass `resolve_nested` parameter
- **`src/fraiseql/__init__.pyi`**: Updated type stubs with new parameter

### 2. Nested Field Resolution
- **`src/fraiseql/core/nested_field_resolver.py`**: Refactored to check `resolve_nested` flag
- **`src/fraiseql/core/graphql_type.py`**: Updated to only use nested resolver when explicitly requested
- Function `should_use_smart_resolver` renamed to `should_use_nested_resolver`

### 3. Documentation
- **`docs/nested-object-resolution.md`**: Comprehensive guide explaining both approaches
- Added extensive docstring examples in `fraise_type.py`
- Updated function documentation with clear behavior explanations

### 4. Tests
- **`tests/test_resolve_nested_parameter.py`**: Tests for both default and explicit behavior
- **`tests/test_nested_tenant_fix_real_db.py`**: Real database tests confirming the fix works

## Migration Guide

### Existing Code (No Changes Required)
All existing code continues to work without modification. The default behavior now assumes embedded data, which is more performant and matches common JSONB view patterns.

### New Applications
```python
# Recommended: Use embedded data (default)
@type(sql_source="v_users_with_org")
class User:
    organization: Organization  # Uses embedded JSONB data

# Only when needed: Separate queries
@type(sql_source="v_departments", resolve_nested=True)
class Department:
    # Will be queried separately when nested
```

## Performance Impact

### Before (Always Attempted Separate Queries)
- Could cause N+1 query problems
- Required tenant_id even for embedded data
- More complex error handling

### After (Default to Embedded)
- Single query for embedded data (better performance)
- No tenant_id required for embedded objects
- Simpler context management
- Explicit opt-in for separate queries when needed

## Breaking Changes

**None.** This is a backward-compatible feature addition.

## Benefits

1. **Performance**: Default behavior avoids N+1 queries
2. **Simplicity**: Embedded data works without additional context
3. **Flexibility**: Can still do separate queries when needed
4. **Explicit Control**: Developers choose resolution strategy per type
5. **Documentation**: Clear guidance on when to use each approach

## Examples

### Multi-Tenant Application (Embedded)
```python
@type(sql_source="v_users_with_tenant")
class User:
    tenant: Tenant  # Embedded - no tenant_id issues

# View includes tenant data in JSONB
CREATE VIEW v_users_with_tenant AS
SELECT id, jsonb_build_object(
    'id', u.id,
    'tenant', jsonb_build_object('id', t.id, 'name', t.name)
) AS data
FROM users u JOIN tenants t ON u.tenant_id = t.id;
```

### Large Related Data (Separate Queries)
```python
@type(sql_source="v_documents", resolve_nested=True)
class Document:
    content: str  # Large text field

@type(sql_source="v_projects")
class Project:
    documents: list[Document]  # Resolved separately to avoid bloat
```

## Related Issues

This feature resolves the "missing a required argument: 'tenant_id'" error that occurred when FraiseQL tried to query nested objects separately even when the data was already embedded in the parent's JSONB column.

## Version

Added in: FraiseQL v0.1.0b16 (estimated)
Compatible with: All previous versions (backward compatible)
