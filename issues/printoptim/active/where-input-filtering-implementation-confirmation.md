# Where Input Filtering Implementation Confirmation

## Context

Following your guidance in `where-input-filtering-implementation-RESPONSE.md`, we've implemented the where input filtering with the dict/object handling pattern you suggested. The implementation is now working successfully (server returning 200 OK responses).

## Current Working Implementation

### 1. Query Implementation with Where Filtering

```python
@fraiseql.query
async def machines(
    info,
    limit: int = 20,
    offset: int = 0,
    where: MachineWhereInput | None = None,
) -> list[Machine]:
    """Retrieve a list of machines with filtering."""

    # Validate limit to prevent performance issues
    if limit > 100:
        raise GraphQLError("Limit cannot exceed 100")

    db = info.context["db"]  # This is a FraiseQLRepository

    # Get tenant_id from request headers (since we don't have custom context)
    request = info.context.get("request")
    tenant_id = "550e8400-e29b-41d4-a716-446655440000"  # Default tenant for now
    if request and hasattr(request, 'headers'):
        tenant_id = request.headers.get("tenant-id", tenant_id)

    # Build filters from where input
    filters = _build_machine_filters(where, tenant_id)

    # Use FraiseQL's find method with filters
    return await db.find("tb_machine",
        **filters,
        limit=limit,
        offset=offset,
        order_by="removed_at DESC NULLS LAST"
    )
```

### 2. Filter Building Functions (Following Your Pattern)

```python
def _build_machine_filters(where: MachineWhereInput | dict | None, tenant_id: str | None) -> dict[str, Any]:
    """Convert machine where input to database filters."""
    filters = {}

    # Always include tenant for security
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

    # Note: Boolean filters (is_current, is_reserved, is_stock, is_unallocated)
    # would need special handling if implemented

    return filters


def _build_allocation_filters(where: AllocationWhereInput | dict | None, tenant_id: str | None) -> dict[str, Any]:
    """Convert allocation where input to database filters."""
    filters = {}

    if tenant_id:
        filters["tenant_id"] = tenant_id

    if not where:
        return filters

    # Handle both AllocationWhereInput instances and dicts
    def get_field(field_name: str):
        if isinstance(where, dict):
            return where.get(field_name)
        else:
            return getattr(where, field_name, None)

    # Define field mappings
    DIRECT_FIELDS = [
        'id', 'machine_id', 'machine_item_id',
        'organizational_unit_id', 'location_id'
    ]

    # Add all direct field filters
    for field in DIRECT_FIELDS:
        value = get_field(field)
        if value is not None:
            filters[field] = value

    # Date filters - these might need special handling for SQL operators
    if get_field('valid_from_gte') is not None:
        filters["valid_from__gte"] = get_field('valid_from_gte')
    if get_field('valid_from_lte') is not None:
        filters["valid_from__lte"] = get_field('valid_from_lte')

    # Note: Boolean filters (is_current, is_past, is_future, is_reserved, is_stock)
    # would need special handling based on dates or status

    return filters
```

### 3. Input Type Definitions

```python
@fraise_input
class MachineWhereInput:
    """Filter input for machine queries."""
    id: uuid.UUID | None = None
    identifier: str | None = None
    model_id: uuid.UUID | None = None
    contract_id: uuid.UUID | None = None
    order_id: uuid.UUID | None = None
    customer_organization_id: uuid.UUID | None = None
    provider_organization_id: uuid.UUID | None = None
    is_current: bool | None = None
    is_reserved: bool | None = None
    is_stock: bool | None = None
    is_unallocated: bool | None = None

@fraise_input
class AllocationWhereInput:
    """Filter input for allocation queries."""
    id: uuid.UUID | None = None
    machine_id: uuid.UUID | None = None
    machine_item_id: uuid.UUID | None = None
    organizational_unit_id: uuid.UUID | None = None
    location_id: uuid.UUID | None = None
    is_current: bool | None = None
    is_past: bool | None = None
    is_future: bool | None = None
    is_reserved: bool | None = None
    is_stock: bool | None = None
    valid_from_gte: date | None = None
    valid_from_lte: date | None = None
```

## Questions for Confirmation

1. **Is our dict/object handling pattern correct?** We followed your suggestion to use a `get_field` helper function that works with both dicts and objects.

2. **Are we using FraiseQLRepository.find() correctly?** We're passing filters as `**filters` keyword arguments.

3. **Boolean filters handling**: We've left these as comments for now. Should we:
   - Pre-compute them in the database view (as you suggested)?
   - Handle them with custom SQL using DatabaseQuery?
   - Or is there another FraiseQL pattern for complex filters?

4. **Date range filters**: For allocation filtering, we're using `valid_from__gte` and `valid_from__lte` as filter keys. Is this the correct pattern for date range filtering with FraiseQL?

5. **safe_create_where_type usage**: We noticed we have this in our filters.py:
   ```python
   from fraiseql.sql.where_generator import safe_create_where_type
   _MachineWhere = safe_create_where_type(Machine)
   ```
   But we're not using these generated WHERE types. Should we be integrating them somehow, or is the manual filter building approach correct for FraiseQL v0.1.0a14?

## Current Status

✅ Where input filtering is working
✅ Server responding with 200 OK to filtered queries
✅ Both dict and object inputs are handled correctly

Please confirm if this implementation follows FraiseQL best practices or if there are any adjustments we should make.
