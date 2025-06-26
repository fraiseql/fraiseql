# Response: FraiseQLRepository API for Query Functions

## The Issue Analysis

Your code looks correct! The `FraiseQLRepository` class **does** have both `find` and `find_one` methods in v0.1.0a14. The error suggests there might be a version mismatch or the repository isn't being instantiated correctly.

## Debugging Steps

### 1. Verify Your FraiseQL Version
```bash
pip show fraiseql
# Should show v0.1.0a14 or higher
```

If you're on an older version:
```bash
pip install --upgrade fraiseql
```

### 2. Check Repository Type in Your Query
Add this debugging code to see what you're actually getting:

```python
@fraiseql.query
async def machines(info, limit: int = 20) -> list[Machine]:
    """Debug repository type."""
    db = info.context["db"]
    
    # Debug information
    print(f"Repository type: {type(db)}")
    print(f"Repository methods: {[method for method in dir(db) if not method.startswith('_')]}")
    print(f"Has find method: {hasattr(db, 'find')}")
    
    # Your actual query
    return await db.find("tv_machine", limit=limit)
```

### 3. Check Your Context Getter
Make sure your `get_context` function is creating the repository correctly:

```python
from fraiseql.db import FraiseQLRepository

async def get_context(request: Request) -> dict[str, Any]:
    """Build GraphQL context."""
    pool = request.app.state.db_pool
    
    # Create repository with proper context
    repo = FraiseQLRepository(pool, context={
        "mode": "development",  # or "production"
        "tenant_id": extract_tenant_id(request)
    })
    
    return {
        "db": repo,
        "request": request,
        "tenant_id": extract_tenant_id(request),
    }
```

## Complete FraiseQLRepository API Reference

### Core Query Methods

#### `find(view_name: str, **kwargs) -> list[Any]`
Find multiple records with filtering:

```python
# Basic usage
machines = await db.find("tv_machine")

# With filters
active_machines = await db.find("tv_machine", 
    status="active",
    tenant_id=tenant_id
)

# With pagination
page_machines = await db.find("tv_machine",
    limit=20,
    offset=0,
    order_by="created_at DESC"
)

# Complex filters
filtered_machines = await db.find("tv_machine",
    tenant_id=tenant_id,
    status="active",
    limit=limit,
    offset=offset,
    order_by="removed_at DESC NULLS LAST"
)
```

#### `find_one(view_name: str, **kwargs) -> Any | None`
Find a single record:

```python
# Find by ID
machine = await db.find_one("tv_machine", id=machine_id)

# Find with multiple filters
machine = await db.find_one("tv_machine", 
    id=machine_id,
    tenant_id=tenant_id
)

# Returns None if not found
machine = await db.find_one("tv_machine", id="nonexistent")
```

#### `run(query: DatabaseQuery) -> list[dict[str, Any]]`
Execute raw SQL (advanced usage):

```python
from fraiseql.db import DatabaseQuery
from psycopg.sql import SQL

# Custom query
query = DatabaseQuery(
    statement=SQL("SELECT * FROM tv_machine WHERE status = %s"),
    params=("active",),
    fetch_result=True
)
results = await db.run(query)
```

## Working Example

Here's a complete working example with proper error handling:

### 1. Your Query Functions
```python
# queries.py
import fraiseql
from uuid import UUID
from typing import Optional

@fraiseql.query
async def machines(
    info,
    limit: int = 20,
    offset: int = 0,
    where: Optional[MachineWhereInput] = None,
) -> list[Machine]:
    """Retrieve a list of machines."""
    try:
        db = info.context["db"]
        tenant_id = info.context.get("tenant_id")
        
        # Verify repository type (remove after debugging)
        if not hasattr(db, 'find'):
            raise RuntimeError(f"Repository {type(db)} doesn't have find method")
        
        # Build filters
        filters = {}
        if tenant_id:
            filters["tenant_id"] = tenant_id
        
        # Add where conditions
        if where:
            if where.status:
                filters["status"] = where.status
            # Add other filters as needed
        
        # Execute query
        return await db.find("tv_machine",
            **filters,
            limit=limit,
            offset=offset,
            order_by="removed_at DESC NULLS LAST"
        )
        
    except Exception as e:
        print(f"Query error: {e}")
        print(f"Repository type: {type(info.context.get('db'))}")
        raise

@fraiseql.query
async def machine(info, id: UUID) -> Optional[Machine]:
    """Get a single machine by ID."""
    db = info.context["db"]
    tenant_id = info.context.get("tenant_id")
    
    filters = {"id": id}
    if tenant_id:
        filters["tenant_id"] = tenant_id
    
    return await db.find_one("tv_machine", **filters)
```

### 2. Context Setup
```python
# context.py
from fraiseql.db import FraiseQLRepository
from fastapi import Request

async def get_context(request: Request) -> dict[str, Any]:
    """Build GraphQL context."""
    # Get pool from app state
    pool = request.app.state.db_pool
    
    # Extract tenant (adjust to your auth logic)
    tenant_id = request.headers.get("x-tenant-id")
    
    # Create repository
    repo = FraiseQLRepository(pool, context={
        "mode": "development",  # or get from env
        "tenant_id": tenant_id
    })
    
    return {
        "db": repo,
        "tenant_id": tenant_id,
        "request": request,
    }
```

### 3. App Setup
```python
# app.py
from fraiseql import create_fraiseql_app
from . import queries  # Import to register @fraiseql.query functions
from .types import Machine, MachineWhereInput
from .context import get_context

app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[Machine, MachineWhereInput],
    context_getter=get_context,
    production=False
)
```

## Mode Behavior

### Development Mode (Default for non-production)
```python
# Returns fully typed objects
machine = await db.find_one("tv_machine", id=machine_id)
print(type(machine))  # <class 'Machine'>
print(machine.name)   # Direct attribute access
```

### Production Mode
```python
# Returns raw dicts for performance
machine = await db.find_one("tv_machine", id=machine_id)
print(type(machine))  # <class 'dict'>
print(machine["data"]["name"])  # Dict access
```

## Common Issues and Solutions

### Issue 1: Repository Not Found
```python
# Check if db is in context
if "db" not in info.context:
    raise RuntimeError("Database not available in context")
```

### Issue 2: Wrong Repository Type
```python
# Check repository type
db = info.context["db"]
if not isinstance(db, FraiseQLRepository):
    raise RuntimeError(f"Expected FraiseQLRepository, got {type(db)}")
```

### Issue 3: Version Mismatch
Make sure you're using FraiseQL v0.1.0a14+ which has the dual-mode repository.

## Testing Your Setup

Try this simple test query:

```python
@fraiseql.query
async def test_repository(info) -> str:
    """Test repository API."""
    db = info.context["db"]
    
    # Check repository
    repo_type = type(db).__name__
    has_find = hasattr(db, 'find')
    has_find_one = hasattr(db, 'find_one')
    
    return f"Repository: {repo_type}, find: {has_find}, find_one: {has_find_one}"
```

Then query:
```graphql
query {
  testRepository
}
```

Expected output: `"Repository: FraiseQLRepository, find: True, find_one: True"`

## If Still Having Issues

1. **Check imports**: Make sure you're importing from the right modules
2. **Verify database connection**: Test that your pool is working
3. **Check logs**: Look for any initialization errors
4. **Minimal reproduction**: Try with the simplest possible setup

The API you're using is correct - `db.find()` and `db.find_one()` are the right methods. The issue is likely in the setup or version.