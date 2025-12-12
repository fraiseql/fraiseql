# WP-027: Add Connection Pooling Configuration to create_fraiseql_app

**Assignee:** ENG-CORE
**Priority:** P1 (Important)
**Estimated Hours:** 8
**Week:** 2
**Dependencies:** None

---

## Objective

Add `connection_pool_size` and related connection pooling parameters to `create_fraiseql_app()` function to enable explicit database connection pool configuration, as documented in journey guides.

**Current State:** `create_fraiseql_app()` does not expose connection pooling parameters. Documentation in `docs/journeys/backend-engineer.md:103-109` references `connection_pool_size` parameter that doesn't exist.

**Target State:** Production-ready connection pooling configuration with sensible defaults and clear documentation.

---

## Problem Statement

**From Journey Doc Verification:**
- `docs/journeys/backend-engineer.md` shows:
  ```python
  app = create_fraiseql_app(
      database_url="postgresql://...",
      connection_pool_size=20  # ❌ DOES NOT EXIST
  )
  ```
- Function signature at `src/fraiseql/fastapi/app.py:155-183` has NO `connection_pool_size` parameter
- Backend engineers following production deployment guides expect to tune connection pooling
- Current implementation may use default pooling from `asyncpg` or `psycopg`, but it's not configurable

---

## Technical Design

### Current Function Signature
```python
# src/fraiseql/fastapi/app.py:155-183
def create_fraiseql_app(
    database_url: str | None = None,
    types: list[type] | None = None,
    mutations: list[type] | None = None,
    queries: list[type] | None = None,
    auto_discover: bool = True,
    config: FraiseQLConfig | None = None,
    auth: AuthBackend | None = None,
    context_getter: Callable | None = None,
    lifespan: Callable | None = None,
    title: str = "FraiseQL API",
    version: str = "1.0.0",
    description: str = "",
    production: bool = False,
    dev_auth_username: str = "admin",
    dev_auth_password: str = "password",
    enable_schema_registry: bool = False,
    app: FastAPI | None = None
) -> FastAPI:
    ...
```

### Proposed Function Signature (Enhanced)
```python
def create_fraiseql_app(
    database_url: str | None = None,
    types: list[type] | None = None,
    mutations: list[type] | None = None,
    queries: list[type] | None = None,
    auto_discover: bool = True,
    config: FraiseQLConfig | None = None,
    auth: AuthBackend | None = None,
    context_getter: Callable | None = None,
    lifespan: Callable | None = None,
    title: str = "FraiseQL API",
    version: str = "1.0.0",
    description: str = "",
    production: bool = False,
    dev_auth_username: str = "admin",
    dev_auth_password: str = "password",
    enable_schema_registry: bool = False,
    app: FastAPI | None = None,
    # NEW PARAMETERS ↓
    connection_pool_size: int | None = None,  # Default: 10 (dev), 20 (prod)
    connection_pool_max_overflow: int | None = None,  # Default: 10
    connection_pool_timeout: float = 30.0,  # Seconds to wait for connection
    connection_pool_recycle: int = 3600,  # Recycle connections after 1 hour
) -> FastAPI:
    ...
```

---

## Implementation Details

### Step 1: Database Connection Pool Configuration

**For asyncpg (current driver):**
```python
# Inside create_fraiseql_app()

# Determine default pool sizes
if connection_pool_size is None:
    connection_pool_size = 20 if production else 10

if connection_pool_max_overflow is None:
    connection_pool_max_overflow = 10

# Create asyncpg pool
import asyncpg

async def _init_db_pool():
    pool = await asyncpg.create_pool(
        database_url,
        min_size=connection_pool_size // 2,  # Min connections
        max_size=connection_pool_size + connection_pool_max_overflow,  # Max connections
        command_timeout=connection_pool_timeout,
        max_inactive_connection_lifetime=connection_pool_recycle,
    )
    return pool

# Store pool in app state
app.state.db_pool = await _init_db_pool()
```

**For psycopg (if applicable):**
```python
from psycopg_pool import AsyncConnectionPool

async def _init_db_pool():
    pool = AsyncConnectionPool(
        database_url,
        min_size=connection_pool_size // 2,
        max_size=connection_pool_size + connection_pool_max_overflow,
        timeout=connection_pool_timeout,
        max_idle=connection_pool_recycle,
    )
    await pool.open()
    return pool
```

### Step 2: Lifespan Management

Ensure pool is properly closed on shutdown:

```python
from contextlib import asynccontextmanager

@asynccontextmanager
async def _db_lifespan(app: FastAPI):
    # Startup
    app.state.db_pool = await _init_db_pool()
    yield
    # Shutdown
    await app.state.db_pool.close()

# Combine with user-provided lifespan if present
if lifespan is not None:
    # Chain lifespans
    ...
```

### Step 3: Context Getter Integration

Update context getter to use connection from pool:

```python
async def _default_context_getter(request: Request):
    async with request.app.state.db_pool.acquire() as conn:
        return {
            "db": conn,
            "request": request,
            # ... other context
        }
```

---

## Files to Modify

### 1. `src/fraiseql/fastapi/app.py`
**Changes:**
- Add 4 new parameters to `create_fraiseql_app()` function signature (lines 155-183)
- Implement connection pool creation with new parameters
- Integrate pool lifecycle with FastAPI lifespan events
- Update docstring to document new parameters

**Lines to modify:** ~155-250 (function definition + implementation)

### 2. `src/fraiseql/db/pool.py` (NEW FILE)
**Purpose:** Centralize connection pool logic

```python
"""Database connection pool management for FraiseQL."""

import asyncpg
from typing import Optional

class DatabasePool:
    """Manages database connection pool lifecycle."""

    def __init__(
        self,
        database_url: str,
        min_size: int = 10,
        max_size: int = 30,
        timeout: float = 30.0,
        recycle: int = 3600,
    ):
        self.database_url = database_url
        self.min_size = min_size
        self.max_size = max_size
        self.timeout = timeout
        self.recycle = recycle
        self._pool: Optional[asyncpg.Pool] = None

    async def open(self):
        """Initialize the connection pool."""
        self._pool = await asyncpg.create_pool(
            self.database_url,
            min_size=self.min_size,
            max_size=self.max_size,
            command_timeout=self.timeout,
            max_inactive_connection_lifetime=self.recycle,
        )

    async def close(self):
        """Close the connection pool."""
        if self._pool:
            await self._pool.close()

    async def acquire(self):
        """Acquire a connection from the pool."""
        if not self._pool:
            raise RuntimeError("Pool not initialized. Call open() first.")
        return await self._pool.acquire()

    async def release(self, connection):
        """Release a connection back to the pool."""
        await self._pool.release(connection)
```

### 3. `docs/reference/api.md` (or similar API reference doc)
**New section:**

```markdown
## Connection Pool Configuration

### Parameters

- **`connection_pool_size`** (int, optional): Number of connections in the pool
  - Default: `10` (development), `20` (production)
  - Recommended: 20-50 for production workloads

- **`connection_pool_max_overflow`** (int, optional): Additional connections beyond pool_size
  - Default: `10`
  - Use for handling traffic spikes

- **`connection_pool_timeout`** (float, optional): Seconds to wait for available connection
  - Default: `30.0`
  - Increase for high-latency databases

- **`connection_pool_recycle`** (int, optional): Seconds before recycling idle connections
  - Default: `3600` (1 hour)
  - Prevents stale connections

### Example Usage

```python
from fraiseql import create_fraiseql_app

# Development (small pool)
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    connection_pool_size=10
)

# Production (tuned pool)
app = create_fraiseql_app(
    database_url="postgresql://db.prod.example.com/mydb",
    connection_pool_size=30,
    connection_pool_max_overflow=20,
    connection_pool_timeout=60.0,
    production=True
)
```

### Tuning Guidelines

| Use Case | Pool Size | Max Overflow | Notes |
|----------|-----------|--------------|-------|
| Development | 5-10 | 5 | Minimal resources |
| Small API (<100 req/s) | 10-20 | 10 | Default settings |
| Medium API (100-500 req/s) | 20-40 | 20 | Most production apps |
| Large API (>500 req/s) | 40-100 | 30 | Monitor connection saturation |

**Warning:** Too many connections can exhaust PostgreSQL `max_connections` (default: 100). Coordinate with DBA.
```

---

## Acceptance Criteria

### Functional Requirements
- ✅ `connection_pool_size` parameter added to `create_fraiseql_app()`
- ✅ Pool size defaults to 10 (dev) and 20 (production)
- ✅ Additional parameters: `max_overflow`, `timeout`, `recycle` implemented
- ✅ Connection pool properly initialized on app startup
- ✅ Connection pool properly closed on app shutdown
- ✅ Pool statistics accessible (e.g., active connections, pool size)

### Documentation Requirements
- ✅ API reference documents all 4 new parameters
- ✅ Production deployment checklist includes pool tuning
- ✅ Journey doc example (`backend-engineer.md`) uses correct parameter
- ✅ Tuning guidelines documented (table with recommendations)

### Testing Requirements
- ✅ Unit test: Pool creation with custom parameters
- ✅ Unit test: Default parameters applied correctly
- ✅ Integration test: Multiple concurrent requests use pooled connections
- ✅ Integration test: Pool exhaustion handled gracefully (timeout error)
- ✅ Load test: 1000 concurrent requests don't exhaust pool

### Backward Compatibility
- ✅ Existing code without pool parameters still works (defaults used)
- ✅ No breaking changes to existing `create_fraiseql_app()` calls

---

## Testing Plan

### Unit Tests (`tests/unit/test_connection_pool.py`)
```python
import pytest
from fraiseql.fastapi import create_fraiseql_app

def test_connection_pool_defaults():
    """Test default pool parameters."""
    app = create_fraiseql_app(
        database_url="postgresql://localhost/test",
        production=False
    )
    assert app.state.db_pool.min_size == 5
    assert app.state.db_pool.max_size == 20  # 10 + 10 overflow

def test_connection_pool_custom_size():
    """Test custom pool size."""
    app = create_fraiseql_app(
        database_url="postgresql://localhost/test",
        connection_pool_size=30,
        connection_pool_max_overflow=15
    )
    assert app.state.db_pool.max_size == 45  # 30 + 15

@pytest.mark.asyncio
async def test_pool_lifecycle():
    """Test pool opens and closes correctly."""
    app = create_fraiseql_app(database_url="postgresql://localhost/test")
    # Pool should be initialized
    assert app.state.db_pool is not None
    # Shutdown
    await app.state.db_pool.close()
```

### Integration Tests (`tests/integration/test_pool_usage.py`)
```python
import pytest
import httpx
from fraiseql.fastapi import create_fraiseql_app

@pytest.mark.asyncio
async def test_concurrent_requests_use_pool():
    """Test that concurrent requests share connection pool."""
    app = create_fraiseql_app(
        database_url="postgresql://localhost/test",
        connection_pool_size=5
    )

    async with httpx.AsyncClient(app=app, base_url="http://test") as client:
        # 20 concurrent requests with pool size 5
        tasks = [client.post("/graphql", json={"query": "{ users { id } }"}) for _ in range(20)]
        responses = await asyncio.gather(*tasks)

        # All requests should succeed (pool handles concurrency)
        assert all(r.status_code == 200 for r in responses)
```

### Load Tests (`tests/load/test_pool_saturation.py`)
```python
import locust

class GraphQLUser(locust.HttpUser):
    @locust.task
    def query_users(self):
        self.client.post("/graphql", json={"query": "{ users { id name } }"})

# Run: locust -f tests/load/test_pool_saturation.py --users 100 --spawn-rate 10
# Verify: No connection timeout errors, pool handles load
```

---

## Implementation Steps

### Step 1: Core Implementation (4 hours)
1. Create `src/fraiseql/db/pool.py` with `DatabasePool` class
2. Update `create_fraiseql_app()` signature with 4 new parameters
3. Integrate pool creation with app lifespan
4. Update context getter to use pooled connections

### Step 2: Testing (2 hours)
1. Write unit tests for pool configuration
2. Write integration tests for concurrent usage
3. Run load tests to verify pool behavior under stress
4. Fix any issues discovered

### Step 3: Documentation (2 hours)
1. Update API reference with new parameters
2. Add tuning guidelines section
3. Update journey doc (`backend-engineer.md`) with correct example
4. Update production checklist to include pool tuning

---

## DO NOT

- ❌ Do not change behavior of existing apps (backward compatibility required)
- ❌ Do not expose raw asyncpg/psycopg pool object to users (abstract it)
- ❌ Do not use global connection pool (must be per-app instance)
- ❌ Do not forget to close pool on shutdown (resource leak)
- ❌ Do not set pool size >100 by default (PostgreSQL has limits)

---

## Success Metrics

### Technical
- Connection pool configurable via `create_fraiseql_app()` parameters
- Pool lifecycle managed automatically (no manual open/close needed)
- Load tests pass: 1000 concurrent requests handled without errors

### User Experience
- Backend engineer can tune pool size in 1 line of code
- Production deployment checklist includes clear pool tuning guidance
- Journey doc example now works as documented

---

## Related Work Packages

- **WP-004:** Backend Engineer Journey (fixes hallucinated `connection_pool_size` parameter)
- **WP-014:** Production Deployment Checklist (add pool tuning section)
- **WP-026:** Benchmark Script (benchmark should test pool behavior)

---

## Notes

**Why This Matters:**
- Connection pooling is critical for production performance
- Backend engineers expect to tune pool size (standard practice)
- Missing pool configuration is a red flag during evaluation
- Documentation claiming feature that doesn't exist damages credibility

**Alternatives Considered:**
1. Use `FraiseQLConfig` for pool settings → Too verbose, `create_fraiseql_app()` is the natural place
2. Remove pool configuration from docs → Weakens production story
3. Only support pool via environment variables → Less flexible, harder to test

**Decision:** Add parameters directly to `create_fraiseql_app()` (this WP)

---

**End of WP-027**
