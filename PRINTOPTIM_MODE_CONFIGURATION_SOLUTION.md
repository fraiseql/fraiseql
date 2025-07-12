# Solution: FraiseQL Development Mode Configuration Issue

**Date:** 2025-07-12  
**Issue:** FraiseQL repository remains in production mode despite configuration attempts  
**Root Cause:** Repository mode is determined at initialization, not from GraphQL context

## The Problem

PrintOptim Backend's FraiseQL repository is stuck in production mode because:

1. The repository's mode is set **once at initialization** in `FraiseQLRepository.__init__()`
2. The mode is determined from the repository's **internal context**, not the GraphQL context
3. Their custom context getter adds mode to the **GraphQL context**, which doesn't affect the repository

## Understanding the Flow

```python
# 1. Repository creation (happens in get_db() dependency)
repository = FraiseQLRepository(pool=pool, context=context)
# Mode is determined HERE from context["mode"] or FRAISEQL_ENV

# 2. GraphQL context creation (happens in build_graphql_context())
graphql_context = {
    "db": repository,  # Repository already has its mode set
    "mode": "development",  # This doesn't affect the repository
    # ... other context
}

# 3. In resolver
db = info.context["db"]  # This repository is already in production mode
```

## Solutions

### Solution 1: Fix the FraiseQLConfig (Recommended)

Ensure your FraiseQLConfig is properly set:

```python
# In app.py or wherever you configure FraiseQL
from fraiseql.fastapi.config import FraiseQLConfig

# Force development mode
fraiseql_config = FraiseQLConfig(
    database_url=settings.database_url,
    environment="development",  # This should work with v0.1.0b11+
    enable_introspection=True,
    enable_playground=True,
)

# Pass config to create_fraiseql_app
app = create_fraiseql_app(
    database_url=settings.database_url,
    config=fraiseql_config,  # Make sure config is passed
    types=[Machine, Model, Order],
    queries=[machines],
)
```

### Solution 2: Set Environment Variable

Set the environment variable before importing FraiseQL:

```python
# At the very top of your app.py or main entry point
import os
os.environ["FRAISEQL_ENV"] = "development"

# Then import FraiseQL
from fraiseql.fastapi import create_fraiseql_app
```

### Solution 3: Custom Database Factory (Advanced)

Create a custom database factory that ensures the correct mode:

```python
from fraiseql.db import FraiseQLRepository
from fraiseql.fastapi.dependencies import get_db_pool

async def get_custom_db() -> FraiseQLRepository:
    pool = get_db_pool()
    # Force development mode in repository context
    context = {"mode": "development", "query_timeout": 30}
    return FraiseQLRepository(pool=pool, context=context)

# Override the default db dependency
app = create_fraiseql_app(
    database_url=settings.database_url,
    types=[Machine, Model, Order],
    queries=[machines],
)

# Override the dependency
from fraiseql.fastapi.dependencies import get_db
app.dependency_overrides[get_db] = get_custom_db
```

### Solution 4: Debug and Verify

Add this to your resolver to debug the actual mode:

```python
@fraiseql.query
async def machines(info, limit: int = 20, offset: int = 0) -> list[Machine]:
    db = info.context["db"]
    
    # Debug the actual repository mode
    print(f"Repository mode: {db.mode}")
    print(f"Repository context: {db.context}")
    print(f"GraphQL context mode: {info.context.get('mode', 'not set')}")
    
    # Check environment variable
    import os
    print(f"FRAISEQL_ENV: {os.getenv('FRAISEQL_ENV', 'not set')}")
    
    result = await db.find("tv_machine", limit=limit, offset=offset)
    print(f"Result type: {type(result[0]) if result else 'empty'}")
    
    return result
```

## Why Raw Dicts Don't Work

The GraphQL error occurs because:

1. In production mode, `db.find()` returns raw dictionaries from the database
2. These dictionaries include all database columns, not just the JSONB data
3. GraphQL expects `Machine` instances or dictionaries matching the Machine schema
4. The raw database row doesn't match the expected GraphQL type structure

## Expected Behavior in Development Mode

When properly configured for development mode:

1. `db.find()` extracts data from the JSONB `data` column
2. Instantiates proper `Machine` objects with nested types
3. Returns typed objects that GraphQL can validate

## Quick Fix (Not Recommended)

If you need a quick workaround while debugging:

```python
@fraiseql.query
async def machines(info, limit: int = 20, offset: int = 0) -> list[dict]:
    db = info.context["db"]
    result = await db.find("tv_machine", limit=limit, offset=offset)
    
    # Extract JSONB data manually
    machines = []
    for row in result:
        if row and isinstance(row, dict) and 'data' in row:
            machines.append(row['data'])
    
    return machines
```

Note: This returns dicts, not Machine instances, but should satisfy GraphQL if the data structure matches.

## Recommendation

1. Update to FraiseQL v0.1.0b11 (if not already)
2. Use Solution 1 (proper FraiseQLConfig)
3. Verify with Solution 4 (debug output)
4. Remove any custom context getters that try to set mode

The key is ensuring the repository is initialized with the correct mode, not trying to change it after creation.