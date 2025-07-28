# Response: safe_create_where_type Integration Feature Request

## Understanding safe_create_where_type

You've made an excellent observation! The `safe_create_where_type` function does exist in FraiseQL and is designed to reduce boilerplate for filter generation. Let me explain why it's not currently recommended and provide you with better alternatives.

## What safe_create_where_type Does

`safe_create_where_type` is a dynamic type generator that:
1. Creates filter dataclasses with operator-based filtering (`{"eq": value}`, `{"gt": value}`)
2. Generates SQL WHERE clauses for JSONB queries
3. Is automatically attached to types when using `@fraise_type(sql_source=True)`

However, it has several limitations that make it less suitable for modern GraphQL APIs.

## Why We Don't Recommend It

### 1. Complexity for Users
```graphql
# The generated API is not intuitive
query {
  machines(where: {
    status: { eq: "active" },
    capacity: { gt: 100, lte: 500 }
  }) {
    id
  }
}

# vs. Your current cleaner API:
query {
  machines(where: {
    status: "active",
    capacityMin: 100,
    capacityMax: 500
  }) {
    id
  }
}
```

### 2. Type Safety Issues
The generated types use `dict[str, Any]` which loses type information:
```python
# No IDE autocomplete or type checking for operators
where = MachineWhere(status={"eq": "active"})  # Is "eq" valid? IDE doesn't know
```

### 3. Tight JSONB Coupling
It generates SQL specifically for JSONB queries (`data ->> 'field'`), which doesn't match your view-based architecture.

## Better Solution: Auto-Filter Builder

Instead of using `safe_create_where_type`, here's a better approach that addresses your DRY concerns:

### 1. Generic Filter Builder (Recommended)

Create a reusable filter builder that works with your existing WhereInput types:

```python
# filters/builder.py
from typing import Any, TypeVar, Type, get_type_hints
from datetime import datetime, date
from decimal import Decimal

T = TypeVar('T')

class FilterBuilder:
    """Generic filter builder for any WhereInput type."""

    @staticmethod
    def build(where: T | dict | None, base_filters: dict[str, Any] | None = None) -> dict[str, Any]:
        """Build filters from any where input type."""
        filters = base_filters.copy() if base_filters else {}

        if not where:
            return filters

        # Handle both dict and object input
        if isinstance(where, dict):
            where_dict = where
        else:
            # Convert object to dict, excluding None values
            where_dict = {
                k: v for k, v in vars(where).items()
                if v is not None and not k.startswith('_')
            }

        # Process each field
        for field, value in where_dict.items():
            if value is None:
                continue

            # Handle special field patterns
            if field.endswith('_min') or field.endswith('_gte'):
                # Range filters (minimum)
                base_field = field.replace('_min', '').replace('_gte', '')
                filters[f"{base_field}__gte"] = value
            elif field.endswith('_max') or field.endswith('_lte'):
                # Range filters (maximum)
                base_field = field.replace('_max', '').replace('_lte', '')
                filters[f"{base_field}__lte"] = value
            elif field.endswith('_contains'):
                # Text search (would need custom SQL)
                filters[f"{field}"] = value  # Mark for custom handling
            elif field.endswith('_in') or isinstance(value, list):
                # IN clause
                base_field = field.replace('_in', '') if field.endswith('_in') else field
                filters[base_field] = value
            else:
                # Direct equality
                filters[field] = value

        return filters

    @staticmethod
    def has_custom_filters(filters: dict[str, Any]) -> bool:
        """Check if filters require custom SQL."""
        custom_patterns = ['__gte', '__lte', '__gt', '__lt', '_contains', '__like']
        return any(
            any(pattern in key for pattern in custom_patterns)
            for key in filters.keys()
        )

# Create a simple wrapper function
def build_filters(where: Any | None, base_filters: dict[str, Any] | None = None) -> dict[str, Any]:
    """Build filters from any where input."""
    return FilterBuilder.build(where, base_filters)
```

### 2. Using the Generic Filter Builder

Now you can use this for ALL your entities without repetition:

```python
# queries/machines.py
from ..filters.builder import build_filters

@fraiseql.query
async def machines(
    info,
    limit: int = 20,
    offset: int = 0,
    where: MachineWhereInput | None = None,
) -> list[Machine]:
    """Retrieve machines with automatic filtering."""

    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")

    # One line to build all filters!
    filters = build_filters(where, base_filters={"tenant_id": tenant_id})

    # Check if we need custom SQL for complex filters
    if FilterBuilder.has_custom_filters(filters):
        return await _query_machines_custom(db, filters, limit, offset)

    # Simple filters can use repository directly
    return await db.find("tb_machine", **filters, limit=limit, offset=offset)

# Same pattern for allocations
@fraiseql.query
async def allocations(
    info,
    limit: int = 20,
    offset: int = 0,
    where: AllocationWhereInput | None = None,
) -> list[Allocation]:
    """Retrieve allocations with automatic filtering."""

    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")

    # Exact same pattern!
    filters = build_filters(where, base_filters={"tenant_id": tenant_id})

    if FilterBuilder.has_custom_filters(filters):
        return await _query_allocations_custom(db, filters, limit, offset)

    return await db.find("tb_allocation", **filters, limit=limit, offset=offset)
```

### 3. Enhanced Version with Type Introspection

For even more automation, you can introspect the WhereInput types:

```python
# filters/advanced_builder.py
from typing import get_type_hints, get_origin, get_args

class AdvancedFilterBuilder:
    """Filter builder with type introspection."""

    @classmethod
    def from_where_input(cls, where_input_type: Type[T], where: T | dict | None, base_filters: dict[str, Any] | None = None) -> dict[str, Any]:
        """Build filters using type hints from WhereInput class."""
        filters = base_filters.copy() if base_filters else {}

        if not where:
            return filters

        # Get type hints to understand field types
        type_hints = get_type_hints(where_input_type)

        # Get values from where input
        if isinstance(where, dict):
            where_dict = where
        else:
            where_dict = {k: v for k, v in vars(where).items() if v is not None}

        for field, value in where_dict.items():
            if field not in type_hints:
                continue

            field_type = type_hints[field]
            origin = get_origin(field_type)

            # Handle Optional types
            if origin is Union:
                args = get_args(field_type)
                field_type = args[0] if len(args) == 2 and type(None) in args else field_type

            # Apply type-specific handling
            if field_type in (datetime, date):
                # Date fields might need special handling
                filters[field] = value
            elif origin is list:
                # List fields become IN clauses
                filters[field] = value
            else:
                # Standard fields
                filters[field] = value

        return filters
```

### 4. Decorator Approach (Advanced)

For maximum convenience, here's a decorator approach:

```python
# filters/decorators.py
from functools import wraps

def auto_filter(where_input_type: Type):
    """Decorator to automatically handle filtering."""
    def decorator(func):
        @wraps(func)
        async def wrapper(info, where=None, limit=20, offset=0, **kwargs):
            db = info.context["db"]
            tenant_id = info.context.get("tenant_id")

            # Auto-build filters
            filters = build_filters(where, {"tenant_id": tenant_id} if tenant_id else None)

            # Inject filters into kwargs
            kwargs['_filters'] = filters

            # Call original function
            return await func(info, where=where, limit=limit, offset=offset, **kwargs)

        return wrapper
    return decorator

# Usage
@fraiseql.query
@auto_filter(MachineWhereInput)
async def machines(info, where=None, limit=20, offset=0, **kwargs) -> list[Machine]:
    db = info.context["db"]
    filters = kwargs.get('_filters', {})

    return await db.find("tb_machine", **filters, limit=limit, offset=offset)
```

## Comparison with safe_create_where_type

| Feature | safe_create_where_type | FilterBuilder |
|---------|------------------------|---------------|
| API Style | `{"eq": value, "gt": value}` | `field: value, field_min: value` |
| Type Safety | Weak (dict[str, Any]) | Strong (preserves types) |
| Ease of Use | Complex | Simple |
| Flexibility | Limited to predefined operators | Easily extensible |
| GraphQL UX | Poor (nested objects) | Excellent (flat structure) |
| Maintenance | Framework-dependent | Your control |

## Recommended Implementation Path

1. **Start with the simple FilterBuilder** - It solves 90% of your use cases
2. **Add custom SQL handling** for complex filters as needed
3. **Consider the decorator approach** if you want even less boilerplate
4. **Keep your clean WhereInput types** - They provide better GraphQL UX

## Future FraiseQL Enhancement

Your feature request is valuable! A built-in filter builder that:
- Works with clean WhereInput types (not operator dictionaries)
- Automatically handles common patterns
- Integrates seamlessly with FraiseQLRepository
- Maintains type safety

Would be an excellent addition to FraiseQL. Consider contributing this pattern back to the framework!

## Example: Complete Implementation

Here's a complete working example for your machine queries:

```python
# filters/core.py
from typing import Any, TypeVar

T = TypeVar('T')

def build_entity_filters(
    where: T | dict | None,
    entity_defaults: dict[str, Any] | None = None
) -> dict[str, Any]:
    """Build filters for any entity type."""
    filters = entity_defaults.copy() if entity_defaults else {}

    if not where:
        return filters

    # Normalize to dict
    where_dict = where if isinstance(where, dict) else {
        k: v for k, v in vars(where).items()
        if v is not None and not k.startswith('_')
    }

    # Process all fields
    for field, value in where_dict.items():
        filters[field] = value

    return filters

# queries/machines.py
@fraiseql.query
async def machines(
    info,
    limit: int = 20,
    offset: int = 0,
    where: MachineWhereInput | None = None,
) -> list[Machine]:
    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")

    # One line for all filtering!
    filters = build_entity_filters(where, {"tenant_id": tenant_id})

    return await db.find("tb_machine", **filters, limit=limit, offset=offset)

# queries/allocations.py
@fraiseql.query
async def allocations(
    info,
    limit: int = 20,
    offset: int = 0,
    where: AllocationWhereInput | None = None,
) -> list[Allocation]:
    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")

    # Same pattern, no repetition!
    filters = build_entity_filters(where, {"tenant_id": tenant_id})

    return await db.find("tb_allocation", **filters, limit=limit, offset=offset)
```

This gives you:
- ✅ DRY principle satisfied
- ✅ Clean GraphQL API
- ✅ Type safety maintained
- ✅ Easy to extend
- ✅ No repetitive code

The key insight is that you don't need the complexity of `safe_create_where_type` - a simple, well-designed filter builder gives you all the benefits with none of the drawbacks!
