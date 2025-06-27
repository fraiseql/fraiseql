# Feature Request: Integrate safe_create_where_type with Current Filtering Pattern

## Context

In the current implementation of where input filtering for FraiseQL v0.1.0a14, we're manually building filter dictionaries for each type. While this works, it requires repetitive code for each entity type.

We noticed that `safe_create_where_type` exists in the codebase but your response indicated we should ignore it. However, the purpose of `safe_create_where_type` seems to be exactly what we need - to avoid having to manually create the implementation for each filter.

## Current Manual Implementation

Currently, we have to write this for every entity:

```python
def _build_machine_filters(where: MachineWhereInput | dict | None, tenant_id: str | None) -> dict[str, Any]:
    """Convert machine where input to database filters."""
    filters = {}

    if tenant_id:
        filters["tenant_id"] = tenant_id

    if not where:
        return filters

    # Handle both MachineWhereInput instances and dicts
    def get_field(field_name: str):
        if isinstance(where, dict):
            return where.get(field_name)
        else:
            return getattr(where, field_name, None)

    # Define field mappings
    DIRECT_FIELDS = [
        'id', 'identifier', 'model_id', 'contract_id', 'order_id',
        'customer_organization_id', 'provider_organization_id'
    ]

    # Add all direct field filters
    for field in DIRECT_FIELDS:
        value = get_field(field)
        if value is not None:
            filters[field] = value

    return filters
```

And we have to repeat similar code for allocations, locations, organizations, etc.

## What We Have in filters.py

```python
from fraiseql.sql.where_generator import safe_create_where_type

# Create WHERE types using safe_create_where_type
_OrganizationWhere = safe_create_where_type(Organization)
_AllocationWhere = safe_create_where_type(Allocation)
_LocationWhere = safe_create_where_type(Location)
_MachineWhere = safe_create_where_type(Machine)
_ModelWhere = safe_create_where_type(Model)
```

## Feature Request

Could `safe_create_where_type` be enhanced or documented to work with the current FraiseQL v0.1.0a14 pattern? Ideally, we'd like something like:

```python
@fraiseql.query
async def machines(
    info,
    limit: int = 20,
    offset: int = 0,
    where: MachineWhereInput | None = None,
) -> list[Machine]:
    """Retrieve a list of machines with filtering."""

    db = info.context["db"]
    tenant_id = # ... get tenant_id

    # Automatically build filters from where input using safe_create_where_type
    filters = convert_where_input_to_filters(where, _MachineWhere, tenant_id=tenant_id)

    return await db.find("tb_machine",
        **filters,
        limit=limit,
        offset=offset,
        order_by="removed_at DESC NULLS LAST"
    )
```

Or even better, if FraiseQLRepository could directly understand WHERE types:

```python
return await db.find("tb_machine",
    where=where,  # Automatically converted using _MachineWhere
    tenant_id=tenant_id,
    limit=limit,
    offset=offset,
    order_by="removed_at DESC NULLS LAST"
)
```

## Benefits

1. **DRY Principle**: No need to manually define field mappings for each entity
2. **Type Safety**: Leverage the type information already in the GraphQL types
3. **Maintainability**: Adding a new field to filtering only requires updating the WhereInput type
4. **Consistency**: All filters would work the same way across all entities
5. **Less Boilerplate**: Significantly reduce repetitive filter-building code

## Questions

1. Is `safe_create_where_type` intended for this use case?
2. If not, is there a plan to provide automatic filter generation from WhereInput types?
3. Could you provide guidance on how to integrate `safe_create_where_type` with the current pattern?
4. Would this be a valuable addition to FraiseQL's feature set?

## Proposed API

Something like:

```python
from fraiseql.filters import auto_filter

@fraiseql.query
@auto_filter(MachineWhereInput, _MachineWhere)  # Decorator approach
async def machines(info, limit: int = 20, offset: int = 0, where: MachineWhereInput | None = None) -> list[Machine]:
    # Filters automatically injected into context or as parameter
    pass
```

Or:

```python
from fraiseql.filters import build_filters

@fraiseql.query
async def machines(info, limit: int = 20, offset: int = 0, where: MachineWhereInput | None = None) -> list[Machine]:
    filters = build_filters(where, Machine)  # Automatically uses safe_create_where_type internally
    return await db.find("tb_machine", **filters, limit=limit, offset=offset)
```

This would make FraiseQL even more powerful by reducing boilerplate while maintaining flexibility for custom filtering logic when needed.
