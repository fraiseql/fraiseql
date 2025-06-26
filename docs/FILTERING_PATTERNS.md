# FraiseQL Filtering Patterns

This guide covers how to implement efficient filtering in FraiseQL queries using `where` input types and repository patterns.

## Table of Contents

1. [The Problem with Manual Filtering](#the-problem-with-manual-filtering)
2. [Where Input Types](#where-input-types)
3. [Basic Filtering Patterns](#basic-filtering-patterns)
4. [Advanced Filtering Patterns](#advanced-filtering-patterns)
5. [Database-Level Filtering](#database-level-filtering)
6. [Performance Considerations](#performance-considerations)
7. [Common Mistakes](#common-mistakes)
8. [Best Practices](#best-practices)

## The Problem with Manual Filtering

### ❌ Inefficient Pattern (What Not to Do)
```python
@fraiseql.query
async def machines(
    info,
    status: str | None = None,
    name: str | None = None,
    created_after: datetime | None = None,
    created_before: datetime | None = None,
    limit: int = 20,
    offset: int = 0
) -> list[Machine]:
    """Poor filtering - too many individual parameters."""
    db = info.context["db"]
    
    # Manual filter building (inefficient)
    filters = {}
    if status:
        filters["status"] = status
    if name:
        filters["name"] = name
    # ... more manual checks
    
    return await db.find("machine_view", **filters, limit=limit, offset=offset)
```

**Problems with this approach:**
- Too many parameters in function signature
- Manual filter building is error-prone
- No validation of filter combinations
- Hard to maintain as filters grow
- Poor GraphQL schema organization

## Where Input Types

### ✅ Efficient Pattern (The Right Way)

#### 1. Define Your Where Input Type
```python
from fraiseql import fraise_input
from datetime import datetime
from uuid import UUID

@fraise_input
class MachineWhereInput:
    """Filtering options for machine queries."""
    
    # Basic equality filters
    id: UUID | None = None
    status: str | None = None
    name: str | None = None
    tenant_id: UUID | None = None
    
    # String matching
    name_contains: str | None = None
    name_starts_with: str | None = None
    
    # Date range filters
    created_after: datetime | None = None
    created_before: datetime | None = None
    removed_after: datetime | None = None
    removed_before: datetime | None = None
    
    # Boolean filters
    is_active: bool | None = None
    has_allocations: bool | None = None
    
    # List filters
    statuses: list[str] | None = None
    ids: list[UUID] | None = None
    
    # Numeric filters
    capacity_min: int | None = None
    capacity_max: int | None = None
```

#### 2. Implement Efficient Query Function
```python
@fraiseql.query
async def machines(
    info,
    where: MachineWhereInput | None = None,
    limit: int = 20,
    offset: int = 0,
    order_by: str = "created_at"
) -> list[Machine]:
    """Get machines with efficient filtering."""
    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")
    
    # Build filters efficiently
    filters = _build_machine_filters(where, tenant_id)
    
    return await db.find("machine_view", 
        **filters,
        limit=limit,
        offset=offset,
        order_by=order_by
    )

def _build_machine_filters(where: MachineWhereInput | None, tenant_id: UUID | None) -> dict[str, Any]:
    """Build database filters from where input."""
    filters = {}
    
    # Always add tenant filtering
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
    if where.is_active is not None:
        filters["is_active"] = where.is_active
    
    # List filters (IN clauses)
    if where.statuses:
        filters["status"] = where.statuses  # Repository handles IN clause
    if where.ids:
        filters["id"] = where.ids
    
    # Date range filters
    if where.created_after:
        filters["created_at__gte"] = where.created_after
    if where.created_before:
        filters["created_at__lte"] = where.created_before
    
    # Numeric range filters
    if where.capacity_min is not None:
        filters["capacity__gte"] = where.capacity_min
    if where.capacity_max is not None:
        filters["capacity__lte"] = where.capacity_max
    
    return filters
```

## Basic Filtering Patterns

### Simple Where Input
```python
@fraise_input
class UserWhereInput:
    email: str | None = None
    role: str | None = None
    is_active: bool | None = None

@fraiseql.query
async def users(info, where: UserWhereInput | None = None) -> list[User]:
    db = info.context["db"]
    
    filters = {}
    if where:
        if where.email:
            filters["email"] = where.email
        if where.role:
            filters["role"] = where.role
        if where.is_active is not None:
            filters["is_active"] = where.is_active
    
    return await db.find("user_view", **filters)
```

### String Matching Patterns
```python
@fraise_input
class ProductWhereInput:
    name: str | None = None
    name_contains: str | None = None
    name_starts_with: str | None = None
    description_contains: str | None = None

@fraiseql.query
async def products(info, where: ProductWhereInput | None = None) -> list[Product]:
    db = info.context["db"]
    
    # For complex string matching, use custom SQL
    if where and (where.name_contains or where.name_starts_with or where.description_contains):
        return await _search_products_with_text(db, where)
    
    # Simple equality filters
    filters = {}
    if where and where.name:
        filters["name"] = where.name
    
    return await db.find("product_view", **filters)

async def _search_products_with_text(db: FraiseQLRepository, where: ProductWhereInput) -> list[Product]:
    """Handle complex text search."""
    from fraiseql.db import DatabaseQuery
    from psycopg.sql import SQL, Identifier, Literal
    
    conditions = []
    params = {}
    
    if where.name_contains:
        conditions.append("name ILIKE %(name_pattern)s")
        params["name_pattern"] = f"%{where.name_contains}%"
    
    if where.name_starts_with:
        conditions.append("name ILIKE %(name_prefix)s")
        params["name_prefix"] = f"{where.name_starts_with}%"
    
    if where.description_contains:
        conditions.append("description ILIKE %(desc_pattern)s")
        params["desc_pattern"] = f"%{where.description_contains}%"
    
    where_clause = " AND ".join(conditions) if conditions else "TRUE"
    
    query = DatabaseQuery(
        statement=SQL(f"SELECT * FROM product_view WHERE {where_clause}"),
        params=params,
        fetch_result=True
    )
    
    results = await db.run(query)
    
    # In development mode, manually instantiate
    if db.mode == "development":
        return [Product(**row["data"]) for row in results]
    return results
```

## Advanced Filtering Patterns

### Nested Where Inputs
```python
@fraise_input
class DateRangeInput:
    after: datetime | None = None
    before: datetime | None = None

@fraise_input
class NumericRangeInput:
    min: float | None = None
    max: float | None = None

@fraise_input
class OrderWhereInput:
    status: str | None = None
    statuses: list[str] | None = None
    customer_id: UUID | None = None
    
    # Nested range filters
    created_at: DateRangeInput | None = None
    updated_at: DateRangeInput | None = None
    total_amount: NumericRangeInput | None = None
    
    # Boolean combinations
    is_paid: bool | None = None
    is_shipped: bool | None = None

@fraiseql.query
async def orders(info, where: OrderWhereInput | None = None) -> list[Order]:
    db = info.context["db"]
    
    filters = _build_order_filters(where)
    return await db.find("order_view", **filters)

def _build_order_filters(where: OrderWhereInput | None) -> dict[str, Any]:
    filters = {}
    
    if not where:
        return filters
    
    # Basic filters
    if where.status:
        filters["status"] = where.status
    if where.statuses:
        filters["status"] = where.statuses  # IN clause
    if where.customer_id:
        filters["customer_id"] = where.customer_id
    if where.is_paid is not None:
        filters["is_paid"] = where.is_paid
    if where.is_shipped is not None:
        filters["is_shipped"] = where.is_shipped
    
    # Date range filters
    if where.created_at:
        if where.created_at.after:
            filters["created_at__gte"] = where.created_at.after
        if where.created_at.before:
            filters["created_at__lte"] = where.created_at.before
    
    if where.updated_at:
        if where.updated_at.after:
            filters["updated_at__gte"] = where.updated_at.after
        if where.updated_at.before:
            filters["updated_at__lte"] = where.updated_at.before
    
    # Numeric range filters
    if where.total_amount:
        if where.total_amount.min is not None:
            filters["total_amount__gte"] = where.total_amount.min
        if where.total_amount.max is not None:
            filters["total_amount__lte"] = where.total_amount.max
    
    return filters
```

### Relationship Filtering
```python
@fraise_input
class PostWhereInput:
    title_contains: str | None = None
    status: str | None = None
    published: bool | None = None
    
    # Author filtering
    author_id: UUID | None = None
    author_email: str | None = None
    author_role: str | None = None
    
    # Tag filtering
    has_tag: str | None = None
    has_any_tags: list[str] | None = None
    has_all_tags: list[str] | None = None

@fraiseql.query
async def posts(info, where: PostWhereInput | None = None) -> list[Post]:
    db = info.context["db"]
    
    # For relationship filtering, use views with joins
    if where and (where.author_email or where.author_role or 
                  where.has_tag or where.has_any_tags or where.has_all_tags):
        return await _filter_posts_with_relationships(db, where)
    
    # Simple filters
    filters = {}
    if where:
        if where.title_contains:
            # This would need custom SQL for ILIKE
            pass
        if where.status:
            filters["status"] = where.status
        if where.published is not None:
            filters["published"] = where.published
        if where.author_id:
            filters["author_id"] = where.author_id
    
    return await db.find("post_view", **filters)
```

## Database-Level Filtering

### Optimized Views for Complex Filters
```sql
-- Create view that pre-computes common filters
CREATE VIEW machine_search_view AS
SELECT 
    m.id,
    m.status,
    m.tenant_id,
    m.created_at,
    m.capacity,
    -- Pre-compute boolean flags
    (m.removed_at IS NULL) as is_active,
    (EXISTS(SELECT 1 FROM allocations a WHERE a.machine_id = m.id)) as has_allocations,
    -- Full-text search vector
    to_tsvector('english', m.name || ' ' || COALESCE(m.description, '')) as search_vector,
    -- JSONB data
    jsonb_build_object(
        'id', m.id,
        'name', m.name,
        'status', m.status,
        'capacity', m.capacity,
        'created_at', m.created_at,
        'removed_at', m.removed_at,
        'is_active', (m.removed_at IS NULL),
        'allocation_count', (
            SELECT COUNT(*) FROM allocations a WHERE a.machine_id = m.id
        )
    ) as data
FROM machines m;

-- Add indexes for performance
CREATE INDEX idx_machine_search_status ON machines(status);
CREATE INDEX idx_machine_search_tenant ON machines(tenant_id);
CREATE INDEX idx_machine_search_active ON machines(tenant_id, (removed_at IS NULL));
CREATE INDEX idx_machine_search_fts ON machines 
USING gin(to_tsvector('english', name || ' ' || COALESCE(description, '')));
```

### Full-Text Search Integration
```python
@fraise_input
class MachineSearchInput:
    # Regular filters
    status: str | None = None
    is_active: bool | None = None
    
    # Text search
    search_query: str | None = None
    name_contains: str | None = None

@fraiseql.query
async def search_machines(info, where: MachineSearchInput | None = None) -> list[Machine]:
    db = info.context["db"]
    
    if where and where.search_query:
        return await _full_text_search_machines(db, where)
    
    # Regular filtering
    filters = {}
    if where:
        if where.status:
            filters["status"] = where.status
        if where.is_active is not None:
            filters["is_active"] = where.is_active
    
    return await db.find("machine_search_view", **filters)

async def _full_text_search_machines(db: FraiseQLRepository, where: MachineSearchInput) -> list[Machine]:
    """Full-text search with ranking."""
    from fraiseql.db import DatabaseQuery
    from psycopg.sql import SQL
    
    query = DatabaseQuery(
        statement=SQL("""
            SELECT *, ts_rank(search_vector, plainto_tsquery('english', %(query)s)) as rank
            FROM machine_search_view
            WHERE search_vector @@ plainto_tsquery('english', %(query)s)
            AND (%(status)s IS NULL OR status = %(status)s)
            AND (%(is_active)s IS NULL OR is_active = %(is_active)s)
            ORDER BY rank DESC, created_at DESC
            LIMIT 50
        """),
        params={
            "query": where.search_query,
            "status": where.status,
            "is_active": where.is_active
        },
        fetch_result=True
    )
    
    results = await db.run(query)
    
    if db.mode == "development":
        return [Machine(**row["data"]) for row in results]
    return results
```

## Performance Considerations

### 1. Use Database Indexes
```sql
-- Index filtering columns (not JSONB data)
CREATE INDEX idx_machines_status_tenant ON machines(tenant_id, status);
CREATE INDEX idx_machines_dates ON machines(created_at, removed_at);
CREATE INDEX idx_machines_capacity ON machines(capacity) WHERE capacity IS NOT NULL;

-- Partial indexes for common filters
CREATE INDEX idx_machines_active ON machines(tenant_id) WHERE removed_at IS NULL;
CREATE INDEX idx_machines_with_allocations ON machines(id) 
WHERE EXISTS(SELECT 1 FROM allocations WHERE machine_id = machines.id);
```

### 2. Limit and Pagination
```python
@fraiseql.query
async def machines(
    info,
    where: MachineWhereInput | None = None,
    limit: int = 20,  # Always have reasonable defaults
    offset: int = 0,
    order_by: str = "created_at"
) -> list[Machine]:
    # Enforce maximum limit
    if limit > 100:
        limit = 100
    
    db = info.context["db"]
    filters = _build_machine_filters(where, info.context.get("tenant_id"))
    
    return await db.find("machine_view", 
        **filters,
        limit=limit,
        offset=offset,
        order_by=order_by
    )
```

### 3. Count Queries for Pagination
```python
@fraise_type
class MachineConnection:
    machines: list[Machine]
    total_count: int
    has_next_page: bool

@fraiseql.query
async def machines_paginated(
    info,
    where: MachineWhereInput | None = None,
    limit: int = 20,
    offset: int = 0
) -> MachineConnection:
    db = info.context["db"]
    filters = _build_machine_filters(where, info.context.get("tenant_id"))
    
    # Get data and count in parallel
    machines_task = db.find("machine_view", **filters, limit=limit + 1, offset=offset)
    count_task = _count_machines(db, filters)
    
    machines, total_count = await asyncio.gather(machines_task, count_task)
    
    has_next_page = len(machines) > limit
    if has_next_page:
        machines = machines[:limit]
    
    return MachineConnection(
        machines=machines,
        total_count=total_count,
        has_next_page=has_next_page
    )

async def _count_machines(db: FraiseQLRepository, filters: dict[str, Any]) -> int:
    """Get count of machines matching filters."""
    from fraiseql.db import DatabaseQuery
    from psycopg.sql import SQL
    
    # Build WHERE clause from filters
    conditions = []
    params = {}
    
    for key, value in filters.items():
        if key not in ['limit', 'offset', 'order_by']:
            conditions.append(f"{key} = %({key})s")
            params[key] = value
    
    where_clause = " AND ".join(conditions) if conditions else "TRUE"
    
    query = DatabaseQuery(
        statement=SQL(f"SELECT COUNT(*) as count FROM machine_view WHERE {where_clause}"),
        params=params,
        fetch_result=True
    )
    
    result = await db.run(query)
    return result[0]["count"] if result else 0
```

## Common Mistakes

### ❌ Mistake 1: Not Using Where Input
```python
# Bad: Individual parameters
@fraiseql.query
async def machines(info, status: str = None, name: str = None, active: bool = None):
    pass
```

### ❌ Mistake 2: Ignoring the Where Input
```python
# Bad: Defining where input but not using it
@fraiseql.query
async def machines(info, where: MachineWhereInput | None = None):
    db = info.context["db"]
    # Not using where at all!
    return await db.find("machine_view")
```

### ❌ Mistake 3: Client-Side Filtering
```python
# Bad: Fetching all data and filtering in Python
@fraiseql.query
async def active_machines(info):
    db = info.context["db"]
    all_machines = await db.find("machine_view")  # Fetches everything!
    return [m for m in all_machines if m.is_active]  # Filters in Python
```

### ❌ Mistake 4: No Validation
```python
# Bad: No validation of filter combinations
@fraiseql.query
async def machines(info, where: MachineWhereInput | None = None):
    if where and where.limit and where.limit > 10000:  # No validation!
        # Could cause performance issues
        pass
```

## Best Practices

1. **Always use where input types** for complex filtering
2. **Separate filter building logic** into helper functions
3. **Add database indexes** for filtering columns
4. **Enforce reasonable limits** to prevent performance issues
5. **Use database-level filtering** whenever possible
6. **Handle text search** with full-text search features
7. **Pre-compute common filters** in database views
8. **Test with large datasets** to validate performance
9. **Document your where inputs** with clear field descriptions
10. **Consider caching** for expensive filter combinations

## Example: Complete Implementation

Here's a complete example showing efficient filtering:

```python
# types.py
@fraise_input
class MachineWhereInput:
    """Filter options for machine queries."""
    
    # Basic filters
    status: str | None = fraise_field(description="Filter by machine status")
    statuses: list[str] | None = fraise_field(description="Filter by multiple statuses")
    is_active: bool | None = fraise_field(description="Filter by active/inactive state")
    
    # Text search
    name_contains: str | None = fraise_field(description="Search in machine name")
    search: str | None = fraise_field(description="Full-text search")
    
    # Date ranges
    created_after: datetime | None = fraise_field(description="Created after date")
    created_before: datetime | None = fraise_field(description="Created before date")
    
    # Numeric ranges
    capacity_min: int | None = fraise_field(description="Minimum capacity")
    capacity_max: int | None = fraise_field(description="Maximum capacity")

# queries.py
@fraiseql.query
async def machines(
    info,
    where: MachineWhereInput | None = None,
    limit: int = 20,
    offset: int = 0,
    order_by: str = "created_at"
) -> list[Machine]:
    """Get machines with efficient filtering."""
    # Validate inputs
    if limit > 100:
        raise GraphQLError("Limit cannot exceed 100")
    
    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")
    
    # Handle full-text search separately
    if where and where.search:
        return await _search_machines(db, where, tenant_id, limit, offset)
    
    # Build filters
    filters = {"tenant_id": tenant_id} if tenant_id else {}
    
    if where:
        # Basic filters
        if where.status:
            filters["status"] = where.status
        if where.statuses:
            filters["status"] = where.statuses  # IN clause
        if where.is_active is not None:
            filters["is_active"] = where.is_active
        
        # Date ranges
        if where.created_after:
            filters["created_at__gte"] = where.created_after
        if where.created_before:
            filters["created_at__lte"] = where.created_before
        
        # Numeric ranges
        if where.capacity_min is not None:
            filters["capacity__gte"] = where.capacity_min
        if where.capacity_max is not None:
            filters["capacity__lte"] = where.capacity_max
        
        # Text search (if name_contains but no full search)
        if where.name_contains and not where.search:
            return await _search_machines_by_name(db, where.name_contains, 
                                                filters, limit, offset)
    
    return await db.find("machine_view", 
        **filters,
        limit=limit,
        offset=offset,
        order_by=order_by
    )
```

This approach provides:
- **Type safety** with input validation
- **Performance** through database-level filtering
- **Flexibility** for complex search scenarios
- **Maintainability** with clear separation of concerns
- **Scalability** with proper indexing and limits

## Dynamic Where Types with FraiseQL Repository

### Overview

FraiseQL now supports dynamic where type generation using `safe_create_where_type`, which creates operator-based filtering that integrates seamlessly with `FraiseQLRepository`.

### Basic Usage

```python
from fraiseql import fraise_type
from fraiseql.sql.where_generator import safe_create_where_type
from fraiseql.db import FraiseQLRepository

# Define your type
@fraise_type
class Product:
    id: UUID
    name: str
    price: Decimal
    stock: int
    created_at: datetime
    is_active: bool
    category: str | None = None

# Generate where type dynamically
ProductWhere = safe_create_where_type(Product)

# Use in queries
@fraiseql.query
async def products(
    info,
    where: ProductWhere | None = None,
    limit: int = 20,
    offset: int = 0
) -> list[Product]:
    """Query products with operator-based filtering."""
    db = info.context["db"]
    return await db.find("product_view", where=where, limit=limit, offset=offset)
```

### Operator-Based Filtering

```python
# Example GraphQL query
query {
  products(
    where: {
      price: { gt: 10.0, lte: 100.0 }
      category: { eq: "electronics" }
      is_active: { eq: true }
      created_at: { gte: "2024-01-01T00:00:00Z" }
    }
  ) {
    id
    name
    price
  }
}
```

### SQL Generation with Type Casting

The where generator automatically handles type casting for JSONB columns:

```python
# Python usage
where = ProductWhere()
where.price = {"gt": 50.0}  # Generates: (data->>'price')::numeric > 50.0
where.is_active = {"eq": True}  # Generates: (data->>'is_active')::boolean = true
where.created_at = {"gte": datetime.now()}  # Generates: (data->>'created_at')::timestamp >= '2024-...'
```

### Repository Mode Support

The repository works in two modes:

1. **Development Mode**: Returns fully instantiated Python objects
```python
repo = FraiseQLRepository(pool, context={"mode": "development"})
products = await repo.find("product_view", where=where)
# products are Product instances
```

2. **Production Mode**: Returns raw dictionaries for performance
```python
repo = FraiseQLRepository(pool, context={"mode": "production"})
products = await repo.find("product_view", where=where)
# products are dictionaries
```

### Complex Filtering Examples

```python
# Range queries
where = ProductWhere()
where.price = {"gte": 10, "lt": 100}  # 10 <= price < 100
where.stock = {"gt": 0}  # In stock items only

# String operations
where.name = {"contains": "Widget"}  # Name contains 'Widget'
where.name = {"startswith": "Super"}  # Name starts with 'Super'

# List operations
where.category = {"in": ["electronics", "gadgets"]}

# Null checks
where.category = {"isnull": False}  # Non-null categories only
```

### Performance Best Practices

1. **Create JSONB indexes**:
```sql
-- GIN index for JSONB data
CREATE INDEX idx_products_data ON products USING gin(data);

-- Expression indexes for frequently queried fields
CREATE INDEX idx_products_price ON products((data->>'price')::numeric);
CREATE INDEX idx_products_category ON products((data->>'category'));
```

2. **Use appropriate operators**:
- Use `eq` for exact matches (most efficient)
- Use range operators (`gt`, `lt`) for numeric/date comparisons
- Use `contains` sparingly on large text fields

3. **Combine with traditional filtering**:
```python
# You can mix where types with additional filters
results = await db.find(
    "product_view",
    where=where,
    tenant_id=tenant_id,  # Additional filter
    limit=50
)
```

### Migration from Manual Filters

If you have existing manual filter implementations:

```python
# Old approach
if where.price_min:
    filters["price__gte"] = where.price_min
if where.price_max:
    filters["price__lte"] = where.price_max

# New approach with dynamic where types
where = ProductWhere()
if price_min:
    where.price = {"gte": price_min}
if price_max:
    where.price = {**where.price, "lte": price_max} if where.price else {"lte": price_max}
```

### Limitations and Considerations

1. **All fields are included**: The generated where type includes all fields from the source type
2. **No custom validation**: Operator validation happens at SQL generation time
3. **JSONB requirement**: Works best with views that have a JSONB `data` column
4. **Type casting overhead**: Casting JSONB to typed values has a small performance cost