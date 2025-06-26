# Response: Type Instantiation Issue in FraiseQL v0.1.0a16

## Issue Identified

The problem is that the FraiseQLRepository is not getting the correct mode setting, causing it to default to production mode which returns raw dictionaries instead of instantiated types.

## Root Causes

There are two potential issues:

### 1. Repository Mode Detection

The repository checks for mode in this order:
1. `context["mode"]` passed to FraiseQLRepository constructor
2. `FRAISEQL_ENV` environment variable
3. Defaults to "production"

Your repository might not be getting the mode in its context.

### 2. Query Implementation Pattern

Based on the error message showing raw database rows, it seems the query might be using direct database access instead of the repository's `find` method.

## Solutions

### Solution 1: Ensure Repository Gets Mode (Recommended)

Update your query to pass mode to the repository:

```python
@fraiseql.query
async def allocations(
    info,
    limit: int = 20,
    offset: int = 0,
    where: AllocationWhereInput | None = None,
) -> list[Allocation]:
    """Retrieve a list of allocations."""
    
    # Get the repository with mode from context
    db = info.context["db"]
    
    # If db doesn't have the right mode, create new instance with mode
    if db.mode != "development":
        pool = db._pool  # Get the connection pool
        db = FraiseQLRepository(pool, context={"mode": "development"})
    
    return await db.find("tv_allocation",
        limit=limit,
        offset=offset,
        order_by="start_date DESC"
    )
```

### Solution 2: Set Environment Variable

Ensure the environment variable is set:

```bash
export FRAISEQL_ENV=development
```

Or in your .env file:
```
FRAISEQL_ENV=development
```

### Solution 3: Fix Context in App Creation

Modify your context getter to include mode:

```python
async def get_context(request):
    """Get context with tenant_id and mode."""
    return {
        "tenant_id": request.headers.get("tenant-id", "550e8400-e29b-41d4-a716-446655440000"),
        "contact_id": request.headers.get("contact-id"),
        "mode": "development",  # Add this line
    }
```

### Solution 4: Debug and Verify

Add this debug query to check the repository mode:

```python
@fraiseql.query
async def debug_repository(info) -> dict[str, str]:
    """Debug repository configuration."""
    db = info.context["db"]
    
    return {
        "repository_mode": db.mode,
        "registry_has_tv_allocation": str("tv_allocation" in db._type_registry),
        "registry_contents": str(list(db._type_registry.keys())),
        "context_keys": str(list(info.context.keys())),
    }
```

## The Correct Pattern

Your queries should:

1. Use `db.find()` or `db.find_one()` methods (not raw SQL)
2. Ensure the repository is in development mode
3. Have types registered with `register_type_for_view()`

Example:

```python
@fraiseql.query
async def allocations(info, limit: int = 20) -> list[Allocation]:
    db = info.context["db"]
    
    # This will return Allocation instances in development mode
    return await db.find("tv_allocation", limit=limit)
```

## Important Note

If you're using raw SQL queries or direct database access instead of the repository methods, type instantiation won't work. Always use:
- `db.find()` for multiple records
- `db.find_one()` for single records
- `db.execute_function()` for mutations

## Quick Fix

The fastest fix is to modify your app creation to ensure the repository gets development mode:

```python
# In your app setup, modify the context getter
async def get_context(request):
    return {
        "tenant_id": request.headers.get("tenant-id"),
        "contact_id": request.headers.get("contact-id"), 
        "mode": "development",  # Add this!
    }
```

This ensures the repository created by FraiseQL will be in development mode and perform type instantiation.

## Next Steps

1. Try Solution 3 (add mode to context) - this is the quickest fix
2. Verify your queries use `db.find()` methods
3. Use the debug query to verify the repository mode
4. Let us know if the issue persists with the exact query implementation you're using