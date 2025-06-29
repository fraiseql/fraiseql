# FraiseQL: Health Endpoints Need Access to Database Pool

## Issue Description
Health check endpoints in PrintOptim Backend need access to the database connection pool managed by FraiseQL, but there's no documented way to access the pool from custom endpoints.

## Current Situation
- FraiseQL v0.1.0b1 manages its own internal database connection pool
- Health endpoints need to check database connectivity and run monitoring queries
- Currently, health endpoints create their own connections, which is inefficient and doesn't reflect the actual app's database state

## Code Example
Current health endpoint implementation (problematic):
```python
async def check_database_health(settings: Settings) -> Dict[str, Any]:
    """Check database connectivity and performance."""
    try:
        # This creates a new connection instead of using the app's pool
        async with await psycopg.AsyncConnection.connect(settings.psycopg_uri) as conn:
            async with conn.cursor() as cur:
                await cur.execute("SELECT 1 as test")
                # ... monitoring queries
```

## Feature Request
Need a way to access FraiseQL's connection pool from custom FastAPI endpoints. Possible solutions:

### Option 1: Expose pool via app.state
```python
# In FraiseQL
app.state.db_pool = pool

# In health endpoint
pool = request.app.state.db_pool
async with pool.connection() as conn:
    # ... use connection
```

### Option 2: Provide a dependency
```python
from fraiseql.dependencies import get_db_pool

async def check_database_health(pool = Depends(get_db_pool)):
    async with pool.connection() as conn:
        # ... use connection
```

### Option 3: Document pool access pattern
If there's already a way to access the pool, please document it in the FraiseQL documentation.

## Use Case
Health endpoints are critical for:
- Kubernetes liveness/readiness probes
- Monitoring systems (Prometheus, Grafana)
- Load balancer health checks
- Debugging database connection issues

## Current Workaround
Creating separate connections in health endpoints, but this:
- Doesn't reflect the actual app's connection pool state
- Creates unnecessary database connections
- May show different connection parameters than the main app

## Impact
- Health endpoints can't accurately report on the app's database connectivity
- Can't monitor connection pool metrics (active connections, idle connections, etc.)
- May cause false positives/negatives in health checks

## Environment
- FraiseQL version: 0.1.0b1
- FastAPI version: 0.115.6
- Use case: Production health monitoring endpoints