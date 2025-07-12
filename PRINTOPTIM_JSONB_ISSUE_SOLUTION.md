# Solution: FraiseQL JSONB Type Instantiation Issue

**Date:** 2025-07-12  
**Issue:** GraphQL queries returning raw dictionaries instead of typed objects  
**Root Cause:** FraiseQL repository running in production mode

## The Problem

PrintOptim Backend is getting raw dictionaries from `db.find()` instead of typed GraphQL objects because the FraiseQL repository is operating in **production mode**. In production mode, FraiseQL prioritizes performance by returning raw database results without type instantiation.

## How FraiseQL Works

FraiseQL has two operational modes:

1. **Production Mode** (default):
   - Returns raw dictionaries from database
   - Optimized for performance
   - No automatic type instantiation

2. **Development Mode**:
   - Instantiates proper typed objects from JSONB data
   - Handles nested objects and type conversions
   - Better developer experience

## Solutions

### 1. Update to FraiseQL v0.1.0b11+ (Recommended)

The latest version includes fixes for configuration handling:

```bash
pip install fraiseql==0.1.0b11
```

### 2. Configure FraiseQL in Development Mode

Update your FraiseQL configuration:

```python
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.fastapi.config import FraiseQLConfig

config = FraiseQLConfig(
    environment="development",  # This enables type instantiation
    enable_introspection=True,
    enable_playground=True,
)

app = create_fraiseql_app(
    database_url=settings.database_url,
    config=config,
    types=[Machine, Model, Order, Contract, MachineItem],
    queries=[machines, machine_by_id],
)
```

### 3. Alternative: Set Environment Variable

```bash
export FRAISEQL_ENV=development
```

### 4. Alternative: Add Mode to Context

If using a custom context getter:

```python
async def get_graphql_context(request: Request) -> dict[str, Any]:
    return {
        "tenant_id": get_tenant_id(request),
        "user": get_current_user(request),
        "mode": "development",  # Force development mode
    }

app = create_fraiseql_app(
    database_url=settings.database_url,
    context_getter=get_graphql_context,
    # ... other config
)
```

## Important Notes

1. **View Names**: Ensure you're using the correct view names (without schema prefix):
   ```python
   # Correct
   register_type_for_view("tv_machine", Machine)
   
   # Incorrect (causes "relation does not exist")
   register_type_for_view("public.tv_machine", Machine)
   ```

2. **JSONB Data Structure**: Your database structure is correct. The `data` column should contain all fields defined in your GraphQL type.

3. **Performance Considerations**: While development mode provides better DX with typed objects, production mode is more performant. Consider your use case when choosing.

## Verification

After applying the fix, your query should return properly typed objects:

```graphql
query GetMachines($limit: Int, $offset: Int) {
    machines(limit: $limit, offset: $offset) {
        id
        machineSerialNumber
        model {
            id
            name
        }
    }
}
```

Should return:
```json
{
    "data": {
        "machines": [
            {
                "id": "1451ff21-0000-0000-0000-000000000001",
                "machineSerialNumber": "test.Machine 001",
                "model": {
                    "id": "33333333-3333-3333-3333-333333333333",
                    "name": "Model XYZ"
                }
            }
        ]
    }
}
```

## Production Deployment

For production deployments where you need typed objects:

1. Consider the performance implications
2. Use caching to mitigate performance impact
3. Or implement a custom resolver that manually instantiates types only when needed

## Summary

The issue is not with your implementation - you're using FraiseQL correctly. The framework just needs to be configured to run in development mode to enable automatic type instantiation from JSONB data.