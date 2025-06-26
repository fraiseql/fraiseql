# Where Input Filtering Implementation

## Context

Following your feedback about unused where input filtering, I've implemented filter building functions for the machine and allocation queries. However, I'm getting an error and would like your review of the implementation approach.

## Current Implementation

### Query with Filtering

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

### Filter Building Function

```python
def _build_machine_filters(where: MachineWhereInput | None, tenant_id: str | None) -> dict[str, Any]:
    """Convert machine where input to database filters."""
    filters = {}
    
    # Always include tenant for security
    if tenant_id:
        filters["tenant_id"] = tenant_id
    
    if not where:
        return filters
    
    # Implement filtering based on actual MachineWhereInput fields
    if where.id is not None:
        filters["id"] = where.id
    if where.identifier is not None:
        filters["identifier"] = where.identifier
    if where.model_id is not None:
        filters["model_id"] = where.model_id
    if where.contract_id is not None:
        filters["contract_id"] = where.contract_id
    if where.order_id is not None:
        filters["order_id"] = where.order_id
    if where.customer_organization_id is not None:
        filters["customer_organization_id"] = where.customer_organization_id
    if where.provider_organization_id is not None:
        filters["provider_organization_id"] = where.provider_organization_id
    
    # Boolean filters - these might need special handling
    # is_current, is_reserved, is_stock, is_unallocated
    # These would typically be computed fields or require specific SQL
    
    return filters
```

### MachineWhereInput Definition

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
```

## Error When Testing

When I test with this query:
```graphql
query {
  machines(where: { modelId: "550e8400-e29b-41d4-a716-446655440001" }) {
    id
  }
}
```

I get the error:
```
"'dict' object has no attribute 'id'"
```

## Questions

1. **Is my filter building approach correct?** I'm manually checking each field on the where input and building a filters dictionary.

2. **How should I handle the where input object?** It seems like the where parameter might be coming in as a dict rather than a MachineWhereInput instance. Should I be converting it first?

3. **What's the correct way to pass filters to db.find()?** I'm using `**filters` to unpack the dictionary, but I'm not sure if FraiseQL expects a different format.

4. **How should I handle complex filters?** For boolean fields like `is_current` or `is_reserved`, these might need special SQL conditions or joins. What's the recommended approach?

5. **Is there a more FraiseQL-idiomatic way to implement filtering?** I noticed you have `safe_create_where_type` in the codebase - should I be using that instead of manual filter building?

## Alternative Approach?

I noticed in our filters.py we have:
```python
from fraiseql.sql.where_generator import safe_create_where_type

# Create WHERE types using safe_create_where_type
_MachineWhere = safe_create_where_type(Machine)
```

Should I be using these generated WHERE types instead of manually building filters? If so, how do I connect the MachineWhereInput to the SQL generation?

Please advise on the best approach for implementing where input filtering with FraiseQL v0.1.0a14.