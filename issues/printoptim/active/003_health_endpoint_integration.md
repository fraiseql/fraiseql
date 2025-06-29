# FraiseQL Health Endpoint Integration

## Problem
Health monitoring endpoints that need database access are failing because they can't properly integrate with FraiseQL's connection management.

## Symptoms
- All health endpoints returning 503 Service Unavailable
- Database health checks failing with connection errors
- Performance monitoring endpoints unable to query database

## Current Implementation
```python
@router.get("/health/database")
async def database_health(settings: Settings = Depends(get_settings)) -> DatabaseStats:
    async with await psycopg.AsyncConnection.connect(settings.psycopg_uri) as conn:
        # This creates a new connection outside FraiseQL's pool
```

## Expected Behavior
Need a way to:
1. Access FraiseQL's connection pool from non-GraphQL endpoints
2. Monitor FraiseQL's connection pool health
3. Get database statistics through FraiseQL's connections

## Questions for FraiseQL Team
1. How can non-GraphQL endpoints access FraiseQL's database connections?
2. Is there a FraiseQL API for monitoring connection pool statistics?
3. Should health endpoints be implemented as GraphQL queries instead?

## Impact
- Cannot monitor application health in production
- No visibility into database connection status
- Kubernetes readiness/liveness probes failing