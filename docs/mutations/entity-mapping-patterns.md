# Mutation Entity Mapping Patterns

FraiseQL v0.1.0b18 introduces enhanced support for mapping `object_data` from PostgreSQL mutation results to entity fields in GraphQL responses. This guide explains the supported patterns and best practices.

## Overview

When a PostgreSQL function returns a `mutation_result` composite type, the `object_data` JSONB field contains the entity data that should be mapped to fields in your mutation success types.

## Supported Patterns

### Pattern 1: Single Entity Mapping

The most common pattern where `object_data` contains a single entity:

```python
@success
class CreateLocationSuccess:
    status: str = "success"
    message: str = ""
    location: Location | None = None

# PostgreSQL returns:
# object_data = {id: "...", name: "Main Warehouse", ...}
# FraiseQL maps entire object_data → location field
```

### Pattern 2: Multiple Named Entities

When `object_data` contains multiple entities with field names:

```python
@success
class UpdateLocationSuccess:
    status: str = "success"
    message: str = ""
    location: Location | None = None
    affected_machines: list[Machine] | None = None

# PostgreSQL returns:
# object_data = {
#   location: {id: "...", name: "..."},
#   affected_machines: [{id: "...", name: "..."}, ...]
# }
# FraiseQL maps object_data.location → location
# FraiseQL maps object_data.affected_machines → affected_machines
```

### Pattern 3: Entity Hint in Metadata

Use `extra_metadata` to provide hints for ambiguous cases:

```sql
-- PostgreSQL function
RETURN (
    v_id,
    ARRAY['created'],
    'success',
    'Machine created successfully.',
    v_machine_data,  -- object_data
    jsonb_build_object('entity', 'machine')  -- hint
);
```

## Automatic Detection

FraiseQL uses intelligent detection to determine how to map `object_data`:

1. **Named Fields First**: If `object_data` contains keys matching field names in the success type, those are mapped directly
2. **Single Entity Detection**: If `object_data` looks like an entity (has id, name, etc.) and there's only one entity field, it maps the entire object
3. **Entity Hints**: Uses `extra_metadata.entity` to find the correct field

## Best Practices

### For Simple Mutations

Return the entity directly in `object_data`:

```sql
-- Good: Simple and clear
v_result.object_data := jsonb_build_object(
    'id', v_location.id,
    'name', v_location.name,
    'identifier', v_location.identifier
);
```

### For Complex Mutations

Return named entities for clarity:

```sql
-- Good: Explicit mapping for multiple entities
v_result.object_data := jsonb_build_object(
    'location', v_location_data,
    'parent_location', v_parent_data,
    'affected_machines', v_machines_array
);
```

### For Apollo Cache Updates

Design your mutations to return all affected entities:

```python
@success
class UpdateMachineLocationSuccess:
    status: str = "success"
    message: str = ""
    machine: Machine | None = None
    old_location: Location | None = None
    new_location: Location | None = None
```

This allows Apollo Client to update its cache for all affected entities in a single mutation.

## Migration from Earlier Versions

If you're upgrading from FraiseQL < v0.1.0b18:

1. **No changes required** for standard single-entity mutations
2. **Test complex mutations** to ensure proper mapping
3. **Add entity hints** if you encounter ambiguous cases

## Troubleshooting

### Entity field returns None

1. Check that your PostgreSQL function populates `object_data`
2. Ensure field names in success type match your intended mapping
3. Use entity hints in `extra_metadata` for clarity

### Wrong field populated

This usually happens with multiple entity fields. Use named entities in `object_data`:

```sql
-- Instead of just returning user data:
v_result.object_data := v_user_data;

-- Be explicit:
v_result.object_data := jsonb_build_object('created_user', v_user_data);
```

## Examples

See the test suite in `tests/test_mutation_entity_mapping.py` for comprehensive examples of all supported patterns.
