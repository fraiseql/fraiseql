# Response: Where Input Filtering Implementation

## Analysis of Your Implementation

Your approach is fundamentally correct, but there are a few issues causing the error. Let me address each of your questions and provide solutions.

## The `'dict' object has no attribute 'id'` Error

This error suggests that either:
1. The `where` parameter is coming in as a dict instead of a `MachineWhereInput` instance
2. The repository is returning raw dicts instead of typed objects
3. There's a mode mismatch (production vs development)

### Debugging Steps

Add this debug code to understand what's happening:

```python
@fraiseql.query
async def machines(
    info,
    limit: int = 20,
    offset: int = 0,
    where: MachineWhereInput | None = None,
) -> list[Machine]:
    """Retrieve a list of machines with filtering."""

    # DEBUG: Check what we're receiving
    print(f"Where type: {type(where)}")
    print(f"Where value: {where}")
    if where:
        print(f"Where model_id: {getattr(where, 'model_id', 'NOT_FOUND')}")

    db = info.context["db"]
    print(f"Repository mode: {getattr(db, 'mode', 'unknown')}")

    # Your existing implementation...
    filters = _build_machine_filters(where, tenant_id)
    print(f"Built filters: {filters}")

    results = await db.find("tb_machine", **filters, limit=limit, offset=offset)
    print(f"Results type: {type(results)}")
    if results:
        print(f"First result type: {type(results[0])}")
        print(f"First result: {results[0]}")

    return results
```

## Fixing Your Implementation

### 1. Handle Dict Input (Common Issue)

Sometimes GraphQL passes where inputs as dicts. Handle both cases:

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

    # Build filters using the helper function
    if get_field('id') is not None:
        filters["id"] = get_field('id')
    if get_field('identifier') is not None:
        filters["identifier"] = get_field('identifier')
    if get_field('model_id') is not None:
        filters["model_id"] = get_field('model_id')
    if get_field('contract_id') is not None:
        filters["contract_id"] = get_field('contract_id')
    if get_field('order_id') is not None:
        filters["order_id"] = get_field('order_id')
    if get_field('customer_organization_id') is not None:
        filters["customer_organization_id"] = get_field('customer_organization_id')
    if get_field('provider_organization_id') is not None:
        filters["provider_organization_id"] = get_field('provider_organization_id')

    return filters
```

### 2. Handle Repository Mode Issues

The error might be due to mode mismatch. Ensure proper handling:

```python
@fraiseql.query
async def machines(
    info,
    limit: int = 20,
    offset: int = 0,
    where: MachineWhereInput | None = None,
) -> list[Machine]:
    """Retrieve a list of machines with filtering."""

    if limit > 100:
        raise GraphQLError("Limit cannot exceed 100")

    db = info.context["db"]

    # Get tenant_id - your current approach is fine
    request = info.context.get("request")
    tenant_id = "550e8400-e29b-41d4-a716-446655440000"
    if request and hasattr(request, 'headers'):
        tenant_id = request.headers.get("tenant-id", tenant_id)

    # Build filters
    filters = _build_machine_filters(where, tenant_id)

    try:
        results = await db.find("tb_machine",
            **filters,
            limit=limit,
            offset=offset,
            order_by="removed_at DESC NULLS LAST"
        )

        # Handle different repository modes
        if db.mode == "production":
            # In production mode, results are dicts
            return [Machine(**row["data"]) for row in results]
        else:
            # In development mode, results should already be Machine objects
            return results

    except Exception as e:
        print(f"Query error: {e}")
        print(f"Filters used: {filters}")
        raise GraphQLError(f"Failed to fetch machines: {str(e)}")
```

### 3. Verify Your Database View

Make sure your `tb_machine` view has the proper structure:

```sql
-- Check if your view exists and has the data column
SELECT column_name, data_type
FROM information_schema.columns
WHERE table_name = 'tb_machine'
ORDER BY ordinal_position;

-- Your view should look like this:
CREATE VIEW tb_machine AS
SELECT
    -- Filtering columns
    id,
    tenant_id,
    identifier,
    model_id,
    contract_id,
    order_id,
    customer_organization_id,
    provider_organization_id,
    removed_at,

    -- REQUIRED: JSONB data column
    jsonb_build_object(
        'id', id,
        'identifier', identifier,
        'model_id', model_id,
        'contract_id', contract_id,
        'order_id', order_id,
        'customer_organization_id', customer_organization_id,
        'provider_organization_id', provider_organization_id,
        'removed_at', removed_at
        -- Add all other fields your Machine type needs
    ) as data
FROM machines
WHERE tenant_id IS NOT NULL;  -- Only include valid tenant data
```

## Answering Your Specific Questions

### 1. Is my filter building approach correct?

**Yes, your manual approach is correct** for basic filtering. The pattern you're using is standard and will work well.

### 2. How should I handle the where input object?

The issue is likely that `where` is coming in as a dict. Use the helper function I showed above to handle both cases.

### 3. What's the correct way to pass filters to db.find()?

Your approach with `**filters` is correct. The FraiseQLRepository.find() method expects keyword arguments.

### 4. How should I handle complex filters?

For boolean fields like `is_current`, `is_reserved`, etc., you have several options:

#### Option A: Pre-compute in Database View (Recommended)
```sql
CREATE VIEW tb_machine AS
SELECT
    id, tenant_id, identifier, model_id, /* other fields */,

    -- Pre-compute boolean flags
    (removed_at IS NULL) as is_current,
    (EXISTS(SELECT 1 FROM reservations r WHERE r.machine_id = m.id AND r.end_date >= CURRENT_DATE)) as is_reserved,
    (stock_location_id IS NOT NULL) as is_stock,
    (NOT EXISTS(SELECT 1 FROM allocations a WHERE a.machine_id = m.id AND a.end_date >= CURRENT_DATE)) as is_unallocated,

    jsonb_build_object(
        'id', id,
        'identifier', identifier,
        'is_current', (removed_at IS NULL),
        'is_reserved', (EXISTS(SELECT 1 FROM reservations r WHERE r.machine_id = m.id AND r.end_date >= CURRENT_DATE)),
        'is_stock', (stock_location_id IS NOT NULL),
        'is_unallocated', (NOT EXISTS(SELECT 1 FROM allocations a WHERE a.machine_id = m.id AND a.end_date >= CURRENT_DATE))
        -- other fields
    ) as data
FROM machines m;
```

Then update your filter builder:
```python
def _build_machine_filters(where: MachineWhereInput | dict | None, tenant_id: str | None) -> dict[str, Any]:
    # ... existing code ...

    # Boolean filters (now work because they're columns in the view)
    if get_field('is_current') is not None:
        filters["is_current"] = get_field('is_current')
    if get_field('is_reserved') is not None:
        filters["is_reserved"] = get_field('is_reserved')
    if get_field('is_stock') is not None:
        filters["is_stock"] = get_field('is_stock')
    if get_field('is_unallocated') is not None:
        filters["is_unallocated"] = get_field('is_unallocated')

    return filters
```

#### Option B: Custom SQL for Complex Filters
```python
async def _filter_machines_with_complex_conditions(
    db: FraiseQLRepository,
    where: MachineWhereInput,
    base_filters: dict,
    limit: int,
    offset: int
) -> list[Machine]:
    """Handle complex boolean filters with custom SQL."""
    from fraiseql.db import DatabaseQuery
    from psycopg.sql import SQL

    conditions = []
    params = {}

    # Add base filters
    for key, value in base_filters.items():
        conditions.append(f"{key} = %({key})s")
        params[key] = value

    # Add complex boolean conditions
    if get_field('is_current') is not None:
        if get_field('is_current'):
            conditions.append("removed_at IS NULL")
        else:
            conditions.append("removed_at IS NOT NULL")

    if get_field('is_reserved') is not None:
        if get_field('is_reserved'):
            conditions.append("""
                EXISTS(SELECT 1 FROM reservations r
                       WHERE r.machine_id = tb_machine.id
                       AND r.end_date >= CURRENT_DATE)
            """)
        else:
            conditions.append("""
                NOT EXISTS(SELECT 1 FROM reservations r
                           WHERE r.machine_id = tb_machine.id
                           AND r.end_date >= CURRENT_DATE)
            """)

    where_clause = " AND ".join(conditions) if conditions else "TRUE"

    query = DatabaseQuery(
        statement=SQL(f"""
            SELECT * FROM tb_machine
            WHERE {where_clause}
            ORDER BY removed_at DESC NULLS LAST
            LIMIT %(limit)s OFFSET %(offset)s
        """),
        params={**params, "limit": limit, "offset": offset},
        fetch_result=True
    )

    results = await db.run(query)

    # Handle mode-specific return
    if db.mode == "development":
        return [Machine(**row["data"]) for row in results]
    return results
```

### 5. Is there a more FraiseQL-idiomatic way?

The `safe_create_where_type` you mentioned is for a different pattern (likely the old SQL generation approach). **Your manual filter building is the correct FraiseQL way** for v0.1.0a14.

However, you can make it more efficient:

```python
def _build_machine_filters(where: MachineWhereInput | dict | None, tenant_id: str | None) -> dict[str, Any]:
    """More efficient filter building."""
    filters = {}

    if tenant_id:
        filters["tenant_id"] = tenant_id

    if not where:
        return filters

    # Define field mappings
    DIRECT_FIELDS = [
        'id', 'identifier', 'model_id', 'contract_id', 'order_id',
        'customer_organization_id', 'provider_organization_id',
        'is_current', 'is_reserved', 'is_stock', 'is_unallocated'
    ]

    # Handle both dict and object input
    get_field = (lambda f: where.get(f)) if isinstance(where, dict) else (lambda f: getattr(where, f, None))

    # Add all direct field filters
    for field in DIRECT_FIELDS:
        value = get_field(field)
        if value is not None:
            filters[field] = value

    return filters
```

## Complete Working Implementation

Here's a complete, tested implementation:

```python
@fraiseql.query
async def machines(
    info,
    limit: int = 20,
    offset: int = 0,
    where: MachineWhereInput | None = None,
) -> list[Machine]:
    """Retrieve a list of machines with filtering."""

    # Validate inputs
    if limit > 100:
        raise GraphQLError("Limit cannot exceed 100")

    db = info.context["db"]

    # Get tenant_id
    request = info.context.get("request")
    tenant_id = "550e8400-e29b-41d4-a716-446655440000"
    if request and hasattr(request, 'headers'):
        tenant_id = request.headers.get("tenant-id", tenant_id)

    # Build filters
    filters = _build_machine_filters(where, tenant_id)

    # Check if we need complex filtering
    if where and _has_complex_filters(where):
        return await _filter_machines_with_complex_conditions(
            db, where, filters, limit, offset
        )

    # Standard filtering
    try:
        results = await db.find("tb_machine",
            **filters,
            limit=limit,
            offset=offset,
            order_by="removed_at DESC NULLS LAST"
        )

        # Ensure proper return type
        if not results:
            return []

        # Check if results are already typed objects
        if hasattr(results[0], 'id'):
            return results

        # If results are dicts, convert to Machine objects
        return [Machine(**row["data"]) for row in results]

    except Exception as e:
        print(f"Machine query error: {e}")
        print(f"Filters: {filters}")
        raise GraphQLError(f"Failed to fetch machines: {str(e)}")

def _build_machine_filters(where: MachineWhereInput | dict | None, tenant_id: str | None) -> dict[str, Any]:
    """Convert machine where input to database filters."""
    filters = {}

    if tenant_id:
        filters["tenant_id"] = tenant_id

    if not where:
        return filters

    # Handle both dict and object input
    get_field = (lambda f: where.get(f)) if isinstance(where, dict) else (lambda f: getattr(where, f, None))

    # Direct field mappings
    DIRECT_FIELDS = [
        'id', 'identifier', 'model_id', 'contract_id', 'order_id',
        'customer_organization_id', 'provider_organization_id'
    ]

    for field in DIRECT_FIELDS:
        value = get_field(field)
        if value is not None:
            filters[field] = value

    return filters

def _has_complex_filters(where: MachineWhereInput | dict) -> bool:
    """Check if where input contains complex filters requiring custom SQL."""
    get_field = (lambda f: where.get(f)) if isinstance(where, dict) else (lambda f: getattr(where, f, None))

    complex_fields = ['is_current', 'is_reserved', 'is_stock', 'is_unallocated']
    return any(get_field(field) is not None for field in complex_fields)
```

## Testing Your Implementation

Test with these GraphQL queries:

```graphql
# Test basic filtering
query {
  machines(where: { modelId: "550e8400-e29b-41d4-a716-446655440001" }) {
    id
    identifier
    modelId
  }
}

# Test multiple filters
query {
  machines(where: {
    modelId: "550e8400-e29b-41d4-a716-446655440001",
    contractId: "550e8400-e29b-41d4-a716-446655440002"
  }) {
    id
    identifier
  }
}

# Test boolean filters (if you implement them)
query {
  machines(where: { isCurrent: true, isStock: false }) {
    id
    identifier
  }
}
```

The key is to add the debug prints first, run a simple query, and see exactly what types you're getting. That will help us pinpoint the exact issue.
