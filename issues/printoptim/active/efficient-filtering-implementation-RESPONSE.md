# Response: Efficient Filtering Implementation

## Current Issues with Your Filtering

Looking at your current implementation, I notice several inefficiencies:

```python
@fraiseql.query
async def machines(
    info,
    limit: int = 20,
    offset: int = 0,
    where: MachineWhereInput | None = None,  # ❌ Defined but not used!
) -> list[Machine]:
    # ❌ You're not actually using the 'where' parameter
    return await db.find("tv_machine",
        tenant_id=tenant_id,
        limit=limit,
        offset=offset,
        order_by="removed_at DESC NULLS LAST"
    )
```

**Problems:**
1. **Unused where parameter** - You define it but never use it
2. **No filter implementation** - All filtering logic is missing
3. **No validation** - No input validation or limits
4. **Inefficient for large datasets** - Always returns all records for a tenant

## Improved Implementation

### 1. Define Your Where Input Type Properly

```python
# types/machine_types.py
from fraiseql import fraise_input, fraise_field
from datetime import datetime
from uuid import UUID

@fraise_input
class MachineWhereInput:
    """Filtering options for machine queries."""

    # Basic equality filters
    id: UUID | None = fraise_field(description="Filter by machine ID")
    status: str | None = fraise_field(description="Filter by machine status")
    name: str | None = fraise_field(description="Filter by exact machine name")

    # Multiple value filters (IN clauses)
    ids: list[UUID] | None = fraise_field(description="Filter by multiple machine IDs")
    statuses: list[str] | None = fraise_field(description="Filter by multiple statuses")

    # String matching
    name_contains: str | None = fraise_field(description="Search in machine name")
    name_starts_with: str | None = fraise_field(description="Machine name starts with")

    # Boolean filters
    is_active: bool | None = fraise_field(description="Filter by active/inactive state")
    has_allocations: bool | None = fraise_field(description="Has current allocations")

    # Date range filters
    created_after: datetime | None = fraise_field(description="Created after date")
    created_before: datetime | None = fraise_field(description="Created before date")
    removed_after: datetime | None = fraise_field(description="Removed after date")
    removed_before: datetime | None = fraise_field(description="Removed before date")

    # Capacity filters
    capacity_min: int | None = fraise_field(description="Minimum capacity")
    capacity_max: int | None = fraise_field(description="Maximum capacity")
```

### 2. Implement Efficient Filter Building

```python
# queries/machine_queries.py
from typing import Any

@fraiseql.query
async def machines(
    info,
    where: MachineWhereInput | None = None,
    limit: int = 20,
    offset: int = 0,
    order_by: str = "created_at"
) -> list[Machine]:
    """Get machines with efficient filtering."""

    # Input validation
    if limit > 100:
        raise GraphQLError("Limit cannot exceed 100 for performance reasons")

    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")

    # Build filters efficiently
    filters = _build_machine_filters(where, tenant_id)

    # Handle complex text search separately for performance
    if where and where.name_contains:
        return await _search_machines_by_name(db, where, filters, limit, offset, order_by)

    return await db.find("tv_machine",
        **filters,
        limit=limit,
        offset=offset,
        order_by=order_by
    )

def _build_machine_filters(where: MachineWhereInput | None, tenant_id: str | None) -> dict[str, Any]:
    """Build database filters from where input."""
    filters = {}

    # Always include tenant filtering for security
    if tenant_id:
        filters["tenant_id"] = tenant_id

    if not where:
        return filters

    # Basic equality filters
    if where.id:
        filters["id"] = where.id
    if where.status:
        filters["status"] = where.status
    if where.name:
        filters["name"] = where.name

    # List filters (will become IN clauses)
    if where.ids:
        filters["id"] = where.ids
    if where.statuses:
        filters["status"] = where.statuses

    # Boolean filters
    if where.is_active is not None:
        if where.is_active:
            filters["removed_at"] = None  # Active machines have no removal date
        else:
            # For inactive machines, use custom SQL
            filters["removed_at__is_not"] = None

    # Date range filters
    if where.created_after:
        filters["created_at__gte"] = where.created_after
    if where.created_before:
        filters["created_at__lte"] = where.created_before
    if where.removed_after:
        filters["removed_at__gte"] = where.removed_after
    if where.removed_before:
        filters["removed_at__lte"] = where.removed_before

    # Numeric range filters
    if where.capacity_min is not None:
        filters["capacity__gte"] = where.capacity_min
    if where.capacity_max is not None:
        filters["capacity__lte"] = where.capacity_max

    return filters

async def _search_machines_by_name(
    db: FraiseQLRepository,
    where: MachineWhereInput,
    base_filters: dict[str, Any],
    limit: int,
    offset: int,
    order_by: str
) -> list[Machine]:
    """Handle name-based text search efficiently."""
    from fraiseql.db import DatabaseQuery
    from psycopg.sql import SQL

    # Build WHERE conditions
    conditions = []
    params = {}

    # Add base filters
    for key, value in base_filters.items():
        if key == "tenant_id":
            conditions.append("tenant_id = %(tenant_id)s")
            params["tenant_id"] = value

    # Add text search
    if where.name_contains:
        conditions.append("name ILIKE %(name_pattern)s")
        params["name_pattern"] = f"%{where.name_contains}%"

    if where.name_starts_with:
        conditions.append("name ILIKE %(name_prefix)s")
        params["name_prefix"] = f"{where.name_starts_with}%"

    # Add other filters
    if where.status:
        conditions.append("status = %(status)s")
        params["status"] = where.status

    if where.is_active is not None:
        if where.is_active:
            conditions.append("removed_at IS NULL")
        else:
            conditions.append("removed_at IS NOT NULL")

    where_clause = " AND ".join(conditions) if conditions else "TRUE"

    query = DatabaseQuery(
        statement=SQL(f"""
            SELECT * FROM tv_machine
            WHERE {where_clause}
            ORDER BY {order_by}
            LIMIT %(limit)s OFFSET %(offset)s
        """),
        params={**params, "limit": limit, "offset": offset},
        fetch_result=True
    )

    results = await db.run(query)

    # Handle mode-specific return types
    if db.mode == "development":
        return [Machine(**row["data"]) for row in results]
    return results
```

### 3. Add Allocation Filtering

```python
@fraise_input
class AllocationWhereInput:
    """Filtering options for allocation queries."""

    # Basic filters
    id: UUID | None = None
    machine_id: UUID | None = None
    status: str | None = None

    # Multiple machine filtering
    machine_ids: list[UUID] | None = None
    statuses: list[str] | None = None

    # Date filters
    start_date_after: datetime | None = None
    start_date_before: datetime | None = None
    end_date_after: datetime | None = None
    end_date_before: datetime | None = None

    # Boolean filters
    is_active: bool | None = None
    is_completed: bool | None = None

@fraiseql.query
async def allocations(
    info,
    where: AllocationWhereInput | None = None,
    limit: int = 20,
    offset: int = 0,
    order_by: str = "start_date"
) -> list[Allocation]:
    """Get allocations with filtering."""

    if limit > 100:
        raise GraphQLError("Limit cannot exceed 100")

    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")

    filters = _build_allocation_filters(where, tenant_id)

    return await db.find("tv_allocation",
        **filters,
        limit=limit,
        offset=offset,
        order_by=order_by
    )

def _build_allocation_filters(where: AllocationWhereInput | None, tenant_id: str | None) -> dict[str, Any]:
    """Build allocation filters."""
    filters = {}

    if tenant_id:
        filters["tenant_id"] = tenant_id

    if not where:
        return filters

    # Basic filters
    if where.id:
        filters["id"] = where.id
    if where.machine_id:
        filters["machine_id"] = where.machine_id
    if where.status:
        filters["status"] = where.status

    # List filters
    if where.machine_ids:
        filters["machine_id"] = where.machine_ids
    if where.statuses:
        filters["status"] = where.statuses

    # Date ranges
    if where.start_date_after:
        filters["start_date__gte"] = where.start_date_after
    if where.start_date_before:
        filters["start_date__lte"] = where.start_date_before
    if where.end_date_after:
        filters["end_date__gte"] = where.end_date_after
    if where.end_date_before:
        filters["end_date__lte"] = where.end_date_before

    # Boolean filters
    if where.is_active is not None:
        if where.is_active:
            filters["end_date__gte"] = datetime.now()  # Active = end date in future
        else:
            filters["end_date__lt"] = datetime.now()   # Inactive = end date in past

    return filters
```

### 4. Add Pagination Support

```python
@fraise_type
class MachineConnection:
    """Paginated machine results."""
    machines: list[Machine]
    total_count: int
    has_next_page: bool
    has_previous_page: bool

@fraiseql.query
async def machines_paginated(
    info,
    where: MachineWhereInput | None = None,
    first: int = 20,
    after: int = 0
) -> MachineConnection:
    """Get paginated machines with total count."""

    if first > 100:
        first = 100

    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")

    filters = _build_machine_filters(where, tenant_id)

    # Get one extra record to check if there's a next page
    machines = await db.find("tv_machine",
        **filters,
        limit=first + 1,
        offset=after,
        order_by="created_at"
    )

    # Get total count for pagination info
    total_count = await _count_machines(db, filters)

    has_next_page = len(machines) > first
    if has_next_page:
        machines = machines[:first]  # Remove the extra record

    has_previous_page = after > 0

    return MachineConnection(
        machines=machines,
        total_count=total_count,
        has_next_page=has_next_page,
        has_previous_page=has_previous_page
    )

async def _count_machines(db: FraiseQLRepository, filters: dict[str, Any]) -> int:
    """Get count of machines matching filters."""
    from fraiseql.db import DatabaseQuery
    from psycopg.sql import SQL

    # Build WHERE clause from filters (excluding limit/offset)
    conditions = []
    params = {}

    for key, value in filters.items():
        if key not in ['limit', 'offset', 'order_by']:
            if key.endswith('__gte'):
                field = key[:-5]
                conditions.append(f"{field} >= %({key})s")
            elif key.endswith('__lte'):
                field = key[:-5]
                conditions.append(f"{field} <= %({key})s")
            elif key.endswith('__is_not'):
                field = key[:-8]
                conditions.append(f"{field} IS NOT %(key_value)s")
                params[f"{key}_value"] = value
                continue
            else:
                if isinstance(value, list):
                    conditions.append(f"{key} = ANY(%({key})s)")
                else:
                    conditions.append(f"{key} = %({key})s")
            params[key] = value

    where_clause = " AND ".join(conditions) if conditions else "TRUE"

    query = DatabaseQuery(
        statement=SQL(f"SELECT COUNT(*) as count FROM tv_machine WHERE {where_clause}"),
        params=params,
        fetch_result=True
    )

    result = await db.run(query)
    return result[0]["count"] if result else 0
```

### 5. Usage Examples

```graphql
# Basic filtering
query {
  machines(where: { status: "active", isActive: true }, limit: 10) {
    id
    name
    status
    capacity
  }
}

# Multiple statuses
query {
  machines(where: { statuses: ["active", "maintenance"] }) {
    id
    name
    status
  }
}

# Date range filtering
query {
  machines(where: {
    createdAfter: "2024-01-01T00:00:00Z",
    createdBefore: "2024-12-31T23:59:59Z"
  }) {
    id
    name
    createdAt
  }
}

# Text search
query {
  machines(where: { nameContains: "printer" }) {
    id
    name
  }
}

# Complex filtering with pagination
query {
  machinesPaginated(
    where: {
      statuses: ["active", "idle"],
      capacityMin: 100,
      hasAllocations: true
    },
    first: 20,
    after: 0
  ) {
    machines {
      id
      name
      capacity
    }
    totalCount
    hasNextPage
    hasPreviousPage
  }
}

# Allocation filtering
query {
  allocations(where: {
    machineIds: ["uuid1", "uuid2"],
    isActive: true,
    startDateAfter: "2024-06-01T00:00:00Z"
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
-- Add indexes for common filter combinations
CREATE INDEX idx_tv_machine_tenant_status ON tv_machine(tenant_id, status);
CREATE INDEX idx_tv_machine_tenant_active ON tv_machine(tenant_id, removed_at)
  WHERE removed_at IS NULL;
CREATE INDEX idx_tv_machine_capacity ON tv_machine(capacity) WHERE capacity IS NOT NULL;
CREATE INDEX idx_tv_machine_dates ON tv_machine(created_at, removed_at);
CREATE INDEX idx_tv_machine_name_text ON tv_machine USING gin(to_tsvector('english', name));

-- Allocation indexes
CREATE INDEX idx_tv_allocation_machine ON tv_allocation(machine_id, status);
CREATE INDEX idx_tv_allocation_dates ON tv_allocation(start_date, end_date);
CREATE INDEX idx_tv_allocation_tenant_active ON tv_allocation(tenant_id, end_date)
  WHERE end_date >= CURRENT_TIMESTAMP;
```

### 2. Optimize Your Views

```sql
-- Create optimized view with pre-computed fields
CREATE VIEW tv_machine_optimized AS
SELECT
    m.*,
    (m.removed_at IS NULL) as is_active,
    (EXISTS(SELECT 1 FROM tv_allocation a
             WHERE a.machine_id = m.id
             AND a.end_date >= CURRENT_TIMESTAMP)) as has_allocations,
    jsonb_build_object(
        'id', m.id,
        'name', m.name,
        'status', m.status,
        'capacity', m.capacity,
        'created_at', m.created_at,
        'removed_at', m.removed_at,
        'is_active', (m.removed_at IS NULL),
        'allocation_count', (
            SELECT COUNT(*) FROM tv_allocation a WHERE a.machine_id = m.id
        )
    ) as data
FROM tv_machine m;
```

## Benefits of This Approach

1. **Performance**: Database-level filtering instead of fetching everything
2. **Type Safety**: Proper GraphQL input types with validation
3. **Flexibility**: Support for complex filter combinations
4. **Scalability**: Pagination and count support
5. **Maintainability**: Clear separation of filter building logic
6. **Security**: Proper tenant isolation
7. **User Experience**: Rich filtering options in GraphQL schema

## Migration Steps

1. **Update your where input types** with the comprehensive filters
2. **Implement filter building functions** as shown above
3. **Add database indexes** for performance
4. **Test with your actual data** to ensure performance is acceptable
5. **Update your GraphQL queries** to use the new filtering options
6. **Consider adding pagination** for better user experience

This approach will make your filtering much more efficient and provide a better experience for your API consumers.
