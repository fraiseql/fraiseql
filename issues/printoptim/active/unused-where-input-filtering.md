# Unused Where Input Filtering Implementation

## Issue Summary

Your current machine and allocation queries define `MachineWhereInput` parameters but don't actually implement the filtering logic. This results in inefficient queries that always return all records for a tenant, regardless of the where input provided.

## Current Problematic Code

### Machine Query (Not Using Where Input)
```python
@fraiseql.query
async def machines(
    info,
    limit: int = 20,
    offset: int = 0,
    where: MachineWhereInput | None = None,  # ❌ Defined but never used!
) -> list[Machine]:
    """Retrieve a list of machines."""
    db = info.context["db"]
    tenant_id = info.context.get("tenant_id", "550e8400-e29b-41d4-a716-446655440000")

    # ❌ The 'where' parameter is completely ignored!
    return await db.find("tv_machine",
        tenant_id=tenant_id,
        limit=limit,
        offset=offset,
        order_by="removed_at DESC NULLS LAST"
    )
```

### Current Issues

1. **Performance Problem**: Always fetches ALL machines for a tenant, then returns only `limit` records
2. **Unused Parameter**: `where` input is defined in GraphQL schema but ignored in implementation
3. **Poor User Experience**: Frontend developers expect filtering to work based on the GraphQL schema
4. **Scalability Issue**: Will become slower as machine count grows
5. **Resource Waste**: Unnecessary database load and memory usage

## Impact on Your Application

### Current Behavior
```graphql
# This query appears to work but doesn't actually filter!
query {
  machines(where: { status: "active", isActive: true }) {
    id
    name
    status
  }
}
```

**What happens**: Returns ALL machines for the tenant (ignoring status and isActive filters)
**What should happen**: Returns only active machines

### Performance Impact

With 1000+ machines per tenant:
- **Current**: `SELECT * FROM tv_machine WHERE tenant_id = ?` (returns 1000+ records)
- **Should be**: `SELECT * FROM tv_machine WHERE tenant_id = ? AND status = 'active' AND removed_at IS NULL` (returns ~200 records)

## Required Implementation

### 1. Update Machine Query

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

    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")

    # ✅ BUILD FILTERS FROM WHERE INPUT
    filters = _build_machine_filters(where, tenant_id)

    return await db.find("tv_machine",
        **filters,  # ✅ Actually use the filters!
        limit=limit,
        offset=offset,
        order_by="removed_at DESC NULLS LAST"
    )

def _build_machine_filters(where: MachineWhereInput | None, tenant_id: str | None) -> dict[str, Any]:
    """Convert where input to database filters."""
    filters = {}

    # Always include tenant for security
    if tenant_id:
        filters["tenant_id"] = tenant_id

    if not where:
        return filters

    # ✅ IMPLEMENT ACTUAL FILTERING LOGIC
    if where.status:
        filters["status"] = where.status
    if where.statuses:  # Multiple statuses (IN clause)
        filters["status"] = where.statuses
    if where.name:
        filters["name"] = where.name
    if where.is_active is not None:
        if where.is_active:
            filters["removed_at"] = None  # Active = not removed
        else:
            # For inactive, you'll need custom SQL or a different approach
            pass

    # Add more filter implementations based on your MachineWhereInput fields

    return filters
```

### 2. Update Allocation Query

```python
@fraiseql.query
async def allocations(
    info,
    limit: int = 20,
    offset: int = 0,
    where: AllocationWhereInput | None = None,
) -> list[Allocation]:
    """Retrieve allocations with filtering."""

    if limit > 100:
        raise GraphQLError("Limit cannot exceed 100")

    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")

    # ✅ BUILD AND USE FILTERS
    filters = _build_allocation_filters(where, tenant_id)

    return await db.find("tv_allocation",
        **filters,
        limit=limit,
        offset=offset,
        order_by="start_date DESC"
    )

def _build_allocation_filters(where: AllocationWhereInput | None, tenant_id: str | None) -> dict[str, Any]:
    """Convert allocation where input to database filters."""
    filters = {}

    if tenant_id:
        filters["tenant_id"] = tenant_id

    if not where:
        return filters

    # ✅ IMPLEMENT ALLOCATION FILTERING
    if where.machine_id:
        filters["machine_id"] = where.machine_id
    if where.machine_ids:  # Multiple machines
        filters["machine_id"] = where.machine_ids
    if where.status:
        filters["status"] = where.status
    if where.is_active is not None:
        # Implement based on your business logic
        # e.g., active = end_date >= current_date
        pass

    return filters
```

### 3. Improve Your Where Input Types

```python
@fraise_input
class MachineWhereInput:
    """Enhanced filtering for machines."""

    # Basic filters
    id: UUID | None = None
    status: str | None = None
    name: str | None = None

    # Multiple value filters
    ids: list[UUID] | None = None
    statuses: list[str] | None = None

    # Text search
    name_contains: str | None = None

    # Boolean filters
    is_active: bool | None = None
    has_allocations: bool | None = None

    # Date filters
    created_after: datetime | None = None
    created_before: datetime | None = None

    # Numeric filters
    capacity_min: int | None = None
    capacity_max: int | None = None

@fraise_input
class AllocationWhereInput:
    """Enhanced filtering for allocations."""

    # Basic filters
    id: UUID | None = None
    machine_id: UUID | None = None
    status: str | None = None

    # Multiple values
    machine_ids: list[UUID] | None = None
    statuses: list[str] | None = None

    # Date ranges
    start_date_after: datetime | None = None
    start_date_before: datetime | None = None
    end_date_after: datetime | None = None
    end_date_before: datetime | None = None

    # Boolean filters
    is_active: bool | None = None
    is_completed: bool | None = None
```

## Testing Your Implementation

### 1. Test Basic Filtering
```graphql
query {
  machines(where: { status: "active" }) {
    id
    name
    status
  }
}
```

### 2. Test Multiple Filters
```graphql
query {
  machines(where: {
    statuses: ["active", "maintenance"],
    isActive: true,
    capacityMin: 100
  }) {
    id
    name
    status
    capacity
  }
}
```

### 3. Test Text Search
```graphql
query {
  machines(where: { nameContains: "printer" }) {
    id
    name
  }
}
```

### 4. Test Date Filtering
```graphql
query {
  allocations(where: {
    startDateAfter: "2024-06-01T00:00:00Z",
    isActive: true
  }) {
    id
    machineId
    startDate
    endDate
  }
}
```

## Performance Improvements

### 1. Add Database Indexes
```sql
-- For machine filtering
CREATE INDEX idx_tv_machine_tenant_status ON tv_machine(tenant_id, status);
CREATE INDEX idx_tv_machine_active ON tv_machine(tenant_id, removed_at) WHERE removed_at IS NULL;
CREATE INDEX idx_tv_machine_capacity ON tv_machine(capacity) WHERE capacity IS NOT NULL;

-- For allocation filtering
CREATE INDEX idx_tv_allocation_machine_status ON tv_allocation(machine_id, status);
CREATE INDEX idx_tv_allocation_dates ON tv_allocation(start_date, end_date);
CREATE INDEX idx_tv_allocation_active ON tv_allocation(tenant_id, end_date) WHERE end_date >= CURRENT_DATE;
```

### 2. Add Query Validation
```python
@fraiseql.query
async def machines(info, where: MachineWhereInput | None = None, limit: int = 20, offset: int = 0) -> list[Machine]:
    # Prevent expensive queries
    if limit > 100:
        raise GraphQLError("Maximum limit is 100")

    # If no filters and large offset, require some filtering
    if not where and offset > 1000:
        raise GraphQLError("Large offset requires filtering")

    # Implementation...
```

## Expected Performance Improvement

### Before (Current)
- **Query**: Always fetches ALL tenant machines
- **Database Load**: High (scanning entire tenant dataset)
- **Memory Usage**: High (loading unnecessary data)
- **Response Time**: Slow (especially with large datasets)

### After (With Filtering)
- **Query**: Fetches only filtered results
- **Database Load**: Low (using indexes and WHERE clauses)
- **Memory Usage**: Low (minimal data transfer)
- **Response Time**: Fast (targeted queries)

### Example Impact
With 1000 machines per tenant:
- **Before**: Returns 1000 machines, sends 20 to client
- **After**: Returns 50 active machines, sends 20 to client
- **Database Load Reduction**: 95%
- **Memory Usage Reduction**: 95%
- **Query Time Improvement**: 80-90% faster

## Action Items

### High Priority (Fix Now)
1. ✅ **Implement filter building functions** for machines and allocations
2. ✅ **Use filters in repository calls** instead of ignoring where inputs
3. ✅ **Add input validation** to prevent performance issues
4. ✅ **Test filtering works** with your actual data

### Medium Priority (Next Sprint)
1. 🔄 **Add database indexes** for common filter combinations
2. 🔄 **Enhance where input types** with more filtering options
3. 🔄 **Add text search capabilities** for name/description fields
4. 🔄 **Implement date range filtering** properly

### Low Priority (Future)
1. 📋 **Add pagination with total counts** for better UX
2. 📋 **Add full-text search** with ranking
3. 📋 **Add filter combination validation** for business rules
4. 📋 **Add query performance monitoring** and alerts

## Risk Assessment

### If Not Fixed
- **Performance**: Queries will become slower as data grows
- **User Experience**: Frontend filtering appears broken
- **Resource Usage**: Unnecessary database and memory load
- **Scalability**: Application won't scale with data growth

### Quick Fix (Minimum Viable)
Just implement basic filtering for `status` and `is_active`:

```python
def _build_machine_filters(where: MachineWhereInput | None, tenant_id: str | None) -> dict[str, Any]:
    filters = {"tenant_id": tenant_id} if tenant_id else {}

    if where:
        if where.status:
            filters["status"] = where.status
        if where.is_active is not None and where.is_active:
            filters["removed_at"] = None

    return filters
```

This alone will provide significant performance improvements for the most common filtering scenarios.
