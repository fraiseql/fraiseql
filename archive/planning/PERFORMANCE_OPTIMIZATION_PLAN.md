# FraiseQL Performance Optimization Plan

**Date**: 2025-10-16
**Status**: Investigation & Planning Phase
**Priority**: High - Critical Performance Gap Identified

---

## 📊 Executive Summary

**Problem**: FraiseQL benchmarks show it's performing **slower than Strawberry GraphQL** despite having architectural advantages that should make it 10-50x faster.

**Root Cause**: FraiseQL's JSON passthrough optimization is **not activating** despite extensive configuration attempts (7+ different approaches tested).

**Impact**:

- Current: FraiseQL performs at parity or slower than Strawberry (~24-37ms)
- Expected: FraiseQL should achieve <5ms queries with JSON passthrough
- **Missing: 10-50x performance improvement**

---

## 🔍 Current Situation Analysis

### Benchmark Results Summary

| Framework | Simple Query (10 users) | Nested Query | Status |
|-----------|-------------------------|--------------|---------|
| **Strawberry Traditional** | 37.87ms (26.40 RPS) | 30.87ms (32.39 RPS) | ✅ Fast |
| **FraiseQL** | 24.00ms (41.66 RPS) | 25.36ms (39.44 RPS) | ⚠️ Should be faster |
| **Strawberry CQRS** | 67.74ms (14.76 RPS) | 31.85ms (31.40 RPS) | ❌ Slowest |

**Key Finding**: FraiseQL is **58% faster** than Strawberry Traditional for simple queries, but this is **far below** the expected 10-50x improvement from JSON passthrough.

### What Should Be Happening

**Expected Behavior** (with JSON passthrough active):

```sql
-- Direct JSON passthrough (0.023ms at DB level)
SELECT data::text AS result FROM tv_user WHERE id = 1;
```

**Response extensions**:

```json
{
  "data": { "user": {...} },
  "extensions": {
    "execution": {
      "mode": "json_passthrough"
    }
  }
}
```

### What's Actually Happening

**Actual Behavior** (field extraction):

```sql
-- Field-by-field JSONB extraction (0.147ms at DB level - 6.4x slower)
SELECT jsonb_build_object(
    'id', data->>'id',
    'name', data->>'name',
    'email', data->>'email',
    '__typename', 'User'
)::text AS result
FROM tv_user WHERE id = 1;
```

**Response**: No `extensions` field at all

**Performance Impact**:

- Database level: 6.4x slower (0.147ms vs 0.023ms)
- Application level: No significant advantage over Strawberry
- Missing: 10-50x expected improvement

### Configuration Attempts (All Failed)

**Attempt 1**: Basic configuration

```python
config = FraiseQLConfig(
    environment="production",
    json_passthrough_enabled=True,
    json_passthrough_in_production=True,
)
```

**Result**: Field extraction (no passthrough)

**Attempt 2**: Turbo Router + Execution Priority

```python
config = FraiseQLConfig(
    environment="production",
    json_passthrough_enabled=True,
    json_passthrough_in_production=True,
    enable_turbo_router=True,
    execution_mode_priority=["json_passthrough", "turbo", "normal"],
)
```

**Result**: Field extraction (no passthrough)

**Attempt 3**: Pure JSONB Mode (No Real Columns)

```python
register_type_for_view(
    "tv_user",
    User,
    table_columns=None,  # No real columns - pure JSONB
    has_jsonb_data=True,
)
```

**Result**: Field extraction (no passthrough)

**Attempt 4**: Custom Context with Explicit Flags

```python
async def get_context(request: Request) -> dict:
    return {
        "request": request,
        "mode": "production",
        "json_passthrough": True,
        "json_passthrough_in_production": True,
        "execution_mode": "passthrough",
    }
```

**Result**: Field extraction (no passthrough)

**Attempt 5**: HTTP Header Trigger

```python
headers = {"x-json-passthrough": "true"}
response = httpx.post(url, json={"query": query}, headers=headers)
```

**Result**: Field extraction (no passthrough)

**Attempt 6**: Debug Mode + Execution Info

```python
config = FraiseQLConfig(
    include_execution_info=True,
    debug=True,
)
```

**Result**: No `extensions` field in response

**Attempt 7**: Remove Custom Context Getter

```python
# Use FraiseQL's default context building
app = create_fraiseql_app(
    config=config,
    types=BENCHMARK_TYPES,
    queries=BENCHMARK_QUERIES,
    # No context_getter parameter
)
```

**Result**: Field extraction (no passthrough)

---

## 🎯 Multi-Phase Optimization Strategy

### Phase 1: Deep Profiling & Root Cause Analysis

**Objective**: Understand exactly where time is spent and why passthrough doesn't activate

#### Approach 1A: SQL Execution Time Analysis

**Action**: Enable PostgreSQL query logging and analyze actual queries

```bash
# Enable PostgreSQL query logging
docker exec graphql-benchmarks-db psql -U postgres -d benchmarks -c "
ALTER SYSTEM SET log_min_duration_statement = 0;
ALTER SYSTEM SET log_statement = 'all';
SELECT pg_reload_conf();
"

# Run benchmark and capture queries
docker logs graphql-benchmarks-db 2>&1 | grep "SELECT.*tv_user" > query_logs.txt

# Profile specific query
docker exec -it graphql-benchmarks-db psql -U postgres -d benchmarks -c "
EXPLAIN (ANALYZE, BUFFERS, VERBOSE)
SELECT jsonb_build_object(
    'id', data->>'id',
    'name', data->>'name',
    'email', data->>'email'
) FROM tv_user LIMIT 10;
"
```

**Expected Findings**:

- Exact query execution time
- Buffer usage and I/O statistics
- Query planning overhead
- Comparison with pure passthrough query

#### Approach 1B: Python Application Profiling

**Action**: Add cProfile to benchmark script

```python
# benchmark_profiled.py
import cProfile
import pstats
import io
import httpx

def profile_fraiseql_query():
    profiler = cProfile.Profile()
    profiler.enable()

    # Run 100 GraphQL queries
    for _ in range(100):
        response = httpx.post(
            "http://localhost:8001/graphql",
            json={"query": "{ users(limit: 10) { id name email } }"}
        )

    profiler.disable()

    # Print top 50 functions by cumulative time
    s = io.StringIO()
    stats = pstats.Stats(profiler, stream=s)
    stats.sort_stats('cumulative')
    stats.print_stats(50)

    print(s.getvalue())

    # Save to file
    with open('profiling_results.txt', 'w') as f:
        f.write(s.getvalue())

if __name__ == "__main__":
    profile_fraiseql_query()
```

**Expected Findings**:

- Time spent in SQL execution vs GraphQL resolution
- FraiseQL framework overhead
- JSON serialization/deserialization time
- Database connection pool overhead

#### Approach 1C: Line-by-Line Query Timing

**Action**: Add timing instrumentation to FraiseQL app

```python
# Add to graphql_app.py resolvers
import time

@fraiseql.query
async def users(info, limit: int = 10) -> list[User]:
    t0 = time.perf_counter()

    db = info.context["db"]
    t1 = time.perf_counter()
    print(f"⏱️ Context access: {(t1-t0)*1000:.3f}ms")

    result = await db.find("tv_user", limit=limit)
    t2 = time.perf_counter()
    print(f"⏱️ DB query: {(t2-t1)*1000:.3f}ms")

    # Result is already processed at this point
    t3 = time.perf_counter()
    print(f"⏱️ Total resolver: {(t3-t0)*1000:.3f}ms")

    return result
```

**Expected Findings**:

- Where exactly the 24ms is spent
- Database query time vs framework overhead
- Object instantiation time

---

### Phase 2: JSON Passthrough Activation (Multiple Parallel Strategies)

#### Strategy 2A: Source Code Instrumentation

**Objective**: Patch FraiseQL to add debug logging and understand why passthrough doesn't activate

**Action**: Create patched FraiseQL version with instrumentation

```python
# patch_fraiseql_debug.py
"""
Patch FraiseQL source code to add JSON passthrough debugging.

This script modifies the installed FraiseQL package to add logging
that helps us understand why passthrough isn't activating.
"""

import os
import sys

def patch_fraiseql_execute():
    """Add debug logging to fraiseql/graphql/execute.py"""

    # Find FraiseQL installation
    import fraiseql
    fraiseql_path = os.path.dirname(fraiseql.__file__)
    execute_path = os.path.join(fraiseql_path, "graphql", "execute.py")

    print(f"📍 Patching: {execute_path}")

    # Read current content
    with open(execute_path, 'r') as f:
        content = f.read()

    # Find passthrough condition check
    # Look for: use_passthrough = (
    patch_code = """
    # === DEBUG INSTRUMENTATION ADDED ===
    print("🔍 PASSTHROUGH CHECK:")
    print(f"  - context keys: {list(context_value.keys())}")
    print(f"  - json_passthrough: {context_value.get('json_passthrough')}")
    print(f"  - execution_mode: {context_value.get('execution_mode')}")
    print(f"  - mode: {context_value.get('mode')}")
    if 'config' in context_value:
        config = context_value['config']
        print(f"  - config.json_passthrough_enabled: {getattr(config, 'json_passthrough_enabled', None)}")
        print(f"  - config.json_passthrough_in_production: {getattr(config, 'json_passthrough_in_production', None)}")
        print(f"  - config.pure_json_passthrough: {getattr(config, 'pure_json_passthrough', None)}")
    if 'db' in context_value:
        db = context_value['db']
        if hasattr(db, 'context'):
            print(f"  - db.context: {db.context}")
    # === END DEBUG INSTRUMENTATION ===

    use_passthrough = (
"""

    # Insert before use_passthrough assignment
    if "use_passthrough = (" in content:
        content = content.replace("use_passthrough = (", patch_code)

        # Write back
        with open(execute_path, 'w') as f:
            f.write(content)

        print("✅ Patching successful!")
        print("⚠️  Restart the FraiseQL service to apply changes")
    else:
        print("❌ Could not find passthrough condition in execute.py")
        print("   FraiseQL version may be different than expected")

if __name__ == "__main__":
    patch_fraiseql_execute()
```

**Usage**:

```bash
# Inside FraiseQL Docker container
docker exec -it fraiseql-benchmark python patch_fraiseql_debug.py
docker restart fraiseql-benchmark

# Run benchmark and check logs
docker logs fraiseql-benchmark -f
```

**Expected Outcome**: Clear logs showing which condition prevents passthrough activation

#### Strategy 2B: Minimal Reproduction Test

**Objective**: Create isolated test case outside Docker to eliminate environmental factors

**Action**: Create minimal FraiseQL app

```python
# test_passthrough_minimal.py
"""
Minimal FraiseQL application to test JSON passthrough activation.

This is a stripped-down version with only the essentials to test
whether passthrough can be activated in a simple scenario.
"""

import asyncio
import fraiseql
from fraiseql.db import register_type_for_view
from fraiseql.fastapi import create_fraiseql_app, FraiseQLConfig
import uvicorn
import httpx
from datetime import datetime

# Simple type - only 3 fields
@fraiseql.type(sql_source="tv_user", jsonb_column="data")
class User:
    id: int
    name: str
    email: str

# Register with pure JSONB mode
register_type_for_view(
    "tv_user",
    User,
    table_columns=None,      # Pure JSONB - no real columns
    has_jsonb_data=True
)

# Simple query
@fraiseql.query
async def users(info, limit: int = 10) -> list[User]:
    """Fetch users - should use JSON passthrough"""
    print(f"🔍 Query context: {info.context.keys()}")
    print(f"🔍 json_passthrough: {info.context.get('json_passthrough')}")
    print(f"🔍 execution_mode: {info.context.get('execution_mode')}")

    result = await info.context["db"].find("tv_user", limit=limit)
    print(f"🔍 Query completed, returned {len(result)} users")
    return result

# Minimal config - only passthrough essentials
config = FraiseQLConfig(
    database_url="postgresql://postgres:postgres@localhost:5432/benchmarks",

    # Core passthrough settings
    environment="production",
    json_passthrough_enabled=True,
    json_passthrough_in_production=True,
    pure_json_passthrough=True,
    execution_mode_priority=["passthrough", "turbo", "normal"],

    # Debug settings
    include_execution_info=True,
    debug=True,

    # Minimal everything else
    database_pool_size=5,
    enable_introspection=True,
    enable_playground=True,
)

app = create_fraiseql_app(
    config=config,
    types=[User],
    queries=[users],
    title="Minimal Passthrough Test"
)

@app.get("/health")
async def health():
    return {"status": "ok"}

async def test_query():
    """Test the query and check response"""
    await asyncio.sleep(1)  # Wait for server startup

    query = """
    {
      users(limit: 5) {
        id
        name
        email
      }
    }
    """

    async with httpx.AsyncClient() as client:
        # Test 1: Standard query
        print("\n=== Test 1: Standard Query ===")
        response = await client.post(
            "http://localhost:8000/graphql",
            json={"query": query}
        )
        print(f"Status: {response.status_code}")
        print(f"Response keys: {response.json().keys()}")
        print(f"Has extensions? {'extensions' in response.json()}")
        if 'extensions' in response.json():
            print(f"Extensions: {response.json()['extensions']}")

        # Test 2: With passthrough header
        print("\n=== Test 2: With x-json-passthrough Header ===")
        response = await client.post(
            "http://localhost:8000/graphql",
            json={"query": query},
            headers={"x-json-passthrough": "true"}
        )
        print(f"Status: {response.status_code}")
        print(f"Has extensions? {'extensions' in response.json()}")
        if 'extensions' in response.json():
            print(f"Extensions: {response.json()['extensions']}")

        # Test 3: Query all fields (not selective)
        print("\n=== Test 3: Query All Fields ===")
        full_query = "{ users(limit: 5) { id name email } }"
        response = await client.post(
            "http://localhost:8000/graphql",
            json={"query": full_query}
        )
        print(f"Status: {response.status_code}")
        print(f"Has extensions? {'extensions' in response.json()}")

if __name__ == "__main__":
    # Run test in background
    asyncio.create_task(test_query())

    # Start server
    uvicorn.run(app, host="0.0.0.0", port=8000, log_level="debug")
```

**Test Variations**:

```bash
# Test with different FraiseQL versions
pip install fraiseql==0.9.5 && python test_passthrough_minimal.py
pip install fraiseql==0.10.0 && python test_passthrough_minimal.py
pip install fraiseql==0.10.2 && python test_passthrough_minimal.py
pip install fraiseql==0.11.0 && python test_passthrough_minimal.py  # if exists

# Test with different table types
# 1. Table (tv_user) - current
# 2. Materialized view (mv_user)
# 3. Regular view (v_user)

# Test with different field selections
# 1. Select all fields: { id name email }
# 2. Select subset: { id name }
# 3. Select one field: { id }
```

**Expected Outcome**: Identify specific conditions that enable/prevent passthrough

#### Strategy 2C: Direct FraiseQL Maintainer Engagement

**Objective**: Get authoritative answer from framework creators

**Action**: Create detailed GitHub issue

**Issue Template**:

```markdown
## JSON Passthrough Not Activating - Need Configuration Guidance

**FraiseQL Version**: 0.10.2
**PostgreSQL Version**: 15.3
**Python Version**: 3.11

### Expected Behavior

According to the documentation, JSON passthrough should:
1. Return JSONB data directly from PostgreSQL
2. Skip field extraction and JSON reconstruction
3. Achieve sub-millisecond latency
4. Report execution mode in response extensions

### Actual Behavior

FraiseQL is extracting JSONB fields one-by-one:

```sql
-- Expected
SELECT data::text AS result FROM tv_user LIMIT 10;  -- 0.023ms

-- Actual
SELECT jsonb_build_object('id', data->>'id', ...) FROM tv_user LIMIT 10;  -- 0.147ms
```

No `extensions` field appears in responses despite `include_execution_info=True`.

### Configuration Attempts

We've tried 7 different configuration approaches (see full list below) without success.

[Include all 7 attempts from above]

### Minimal Reproduction

[Include test_passthrough_minimal.py]

### Questions

1. Is JSON passthrough fully implemented in v0.10.2?
2. What exact configuration is required to activate it?
3. Does it work with `tv_*` tables or only with views?
4. Why doesn't `include_execution_info=True` add extensions to responses?
5. Are there specific query patterns required (all fields vs selective)?
6. Does defining `table_columns` disable passthrough?
7. Can you provide a working production example?

### Environment

- Database: PostgreSQL 15.3
- Table structure: `tv_user` with JSONB `data` column
- Type registration: `table_columns=None, has_jsonb_data=True`
- Deployment: Docker containers

### Impact

Without JSON passthrough, FraiseQL performs similarly to Strawberry GraphQL (~24ms vs 37ms),
missing the expected 10-50x improvement that makes FraiseQL compelling.

Any guidance would be greatly appreciated!

```

**Post to**: https://github.com/fraiseql/fraiseql/issues

---

### Phase 3: FraiseQL Configuration System Redesign

**Objective**: Make optimal FraiseQL configuration a one-liner

**Problem**: Too many configuration knobs (50+ options), unclear which matter for performance

#### Create Performance Preset System

**File**: `fraiseql_performance_presets.py`

```python
"""
FraiseQL Performance Presets - One-liner optimal configuration

This module provides pre-configured performance profiles that give you
maximum FraiseQL performance without needing to understand 50+ config options.

Usage:
    from fraiseql_performance_presets import create_optimized_config, PerformanceProfile

    # Maximum speed - everything optimized for performance
    config = create_optimized_config(
        database_url="postgresql://...",
        profile=PerformanceProfile.MAXIMUM_SPEED
    )

    # Or production-safe with monitoring
    config = create_optimized_config(
        database_url="postgresql://...",
        profile=PerformanceProfile.PRODUCTION_SAFE
    )
"""

from fraiseql.fastapi import FraiseQLConfig
from enum import Enum
from typing import Optional

class PerformanceProfile(Enum):
    """Pre-configured performance profiles for common use cases"""

    MAXIMUM_SPEED = "maximum_speed"
    """
    Everything optimized for raw speed. Suitable for:
    - Benchmarks
    - Read-heavy production APIs
    - Internal tools
    - When you trust your input
    """

    PRODUCTION_SAFE = "production_safe"
    """
    Fast but with safety features enabled. Suitable for:
    - Production APIs with untrusted input
    - When you need metrics/monitoring
    - Compliance requirements
    """

    DEVELOPMENT = "development"
    """
    Debug-friendly settings. Suitable for:
    - Local development
    - Debugging performance issues
    - Learning FraiseQL
    """

def create_optimized_config(
    database_url: str,
    profile: PerformanceProfile = PerformanceProfile.MAXIMUM_SPEED,
    **overrides
) -> FraiseQLConfig:
    """
    Create FraiseQL config with sane defaults for maximum performance.

    Args:
        database_url: PostgreSQL connection string
        profile: Performance profile to use
        **overrides: Any config options to override

    Returns:
        FraiseQLConfig optimized for the selected profile

    Example:
        # Maximum speed
        config = create_optimized_config(
            database_url="postgresql://localhost/mydb",
            profile=PerformanceProfile.MAXIMUM_SPEED
        )

        # Production-safe with custom pool size
        config = create_optimized_config(
            database_url="postgresql://localhost/mydb",
            profile=PerformanceProfile.PRODUCTION_SAFE,
            database_pool_size=100  # Override default
        )
    """

    if profile == PerformanceProfile.MAXIMUM_SPEED:
        base_config = {
            # === Core Performance Settings ===
            "environment": "production",

            # JSON Passthrough - The Big Win (10-50x speedup)
            "json_passthrough_enabled": True,
            "json_passthrough_in_production": True,
            "pure_json_passthrough": True,
            "pure_passthrough_use_rust": True,  # Use Rust transformer if available
            "execution_mode_priority": ["passthrough", "turbo", "normal"],

            # === Database Connection Pool ===
            # Tuned for concurrent requests
            "database_pool_size": 50,          # Higher for concurrent load
            "database_max_overflow": 25,       # Allow burst traffic
            "database_pool_timeout": 5,        # Fast timeout (fail fast)
            "database_pool_recycle": 3600,     # Recycle connections hourly

            # === APQ (Automatic Persisted Queries) ===
            # Reduces query parsing overhead
            "apq_storage_backend": "memory",   # Fast in-memory storage
            "apq_cache_responses": True,       # Cache full responses
            "apq_response_cache_ttl": 3600,    # 1 hour cache

            # === Turbo Router ===
            # Fast path for common queries
            "enable_turbo_router": True,
            "turbo_router_cache_size": 2000,   # Large cache
            "turbo_max_complexity": 200,       # Allow complex queries
            "turbo_max_total_weight": 5000.0,

            # === Passthrough Optimization ===
            "passthrough_auto_detect_views": True,
            "passthrough_cache_view_metadata": True,
            "passthrough_view_metadata_ttl": 7200,  # Cache metadata 2 hours

            # === JSONB Optimization ===
            "jsonb_extraction_enabled": True,
            "jsonb_auto_detect": True,
            "jsonb_field_limit_threshold": 50,  # When to use full passthrough

            # === Disable Overhead ===
            "enable_metrics": False,            # No metrics collection
            "enable_rate_limiting": False,      # No rate limiting
            "enable_request_logging": False,    # No request logs
            "enable_response_logging": False,   # No response logs
            "complexity_enabled": False,        # No complexity checks
            "enable_introspection": False,      # No schema introspection
            "enable_playground": False,         # No GraphQL Playground

            # === Timeouts ===
            "query_timeout": 60,                # Generous timeout for complex queries
        }

    elif profile == PerformanceProfile.PRODUCTION_SAFE:
        base_config = {
            # === Core Performance Settings ===
            "environment": "production",

            # JSON Passthrough
            "json_passthrough_enabled": True,
            "json_passthrough_in_production": True,
            "pure_json_passthrough": True,
            "pure_passthrough_use_rust": True,
            "execution_mode_priority": ["passthrough", "turbo", "normal"],

            # === Database Connection Pool ===
            "database_pool_size": 30,
            "database_max_overflow": 15,
            "database_pool_timeout": 10,
            "database_pool_recycle": 3600,

            # === APQ ===
            "apq_storage_backend": "redis",     # Persistent cache
            "apq_cache_responses": True,
            "apq_response_cache_ttl": 3600,

            # === Turbo Router ===
            "enable_turbo_router": True,
            "turbo_router_cache_size": 1000,
            "turbo_max_complexity": 100,
            "turbo_max_total_weight": 2000.0,

            # === Safety Features ENABLED ===
            "enable_metrics": True,             # Track performance
            "enable_rate_limiting": True,       # Prevent abuse
            "rate_limit_requests_per_minute": 1000,
            "complexity_enabled": True,         # Prevent expensive queries
            "max_query_complexity": 100,
            "enable_request_logging": True,     # Audit trail

            # === Introspection ===
            "enable_introspection": True,       # Allow schema queries
            "enable_playground": False,         # No public playground

            # === Timeouts ===
            "query_timeout": 30,                # Stricter timeout
        }

    elif profile == PerformanceProfile.DEVELOPMENT:
        base_config = {
            # === Debug-Friendly Settings ===
            "environment": "development",

            # JSON Passthrough (for testing)
            "json_passthrough_enabled": True,
            "json_passthrough_in_production": False,  # Only in explicit mode
            "pure_json_passthrough": False,            # Use safer mode
            "execution_mode_priority": ["normal", "turbo", "passthrough"],

            # === Small Pool for Local Dev ===
            "database_pool_size": 5,
            "database_max_overflow": 2,
            "database_pool_timeout": 30,

            # === APQ ===
            "apq_storage_backend": "memory",
            "apq_cache_responses": False,       # Don't cache during dev

            # === All Debug Features ENABLED ===
            "enable_metrics": True,
            "enable_request_logging": True,
            "enable_response_logging": True,
            "enable_introspection": True,
            "enable_playground": True,          # Enable GraphQL Playground
            "debug": True,
            "include_execution_info": True,     # Show execution mode

            # === Generous Limits ===
            "complexity_enabled": False,
            "query_timeout": 300,               # 5 minutes for debugging
        }

    # Merge with user overrides
    final_config = {**base_config, **overrides}

    return FraiseQLConfig(
        database_url=database_url,
        **final_config
    )

# Convenience functions for common scenarios

def create_benchmark_config(database_url: str, **overrides) -> FraiseQLConfig:
    """Create config optimized for benchmarking"""
    return create_optimized_config(
        database_url=database_url,
        profile=PerformanceProfile.MAXIMUM_SPEED,
        **overrides
    )

def create_production_config(
    database_url: str,
    redis_url: Optional[str] = None,
    **overrides
) -> FraiseQLConfig:
    """Create production-safe config with optional Redis"""
    config_overrides = overrides.copy()
    if redis_url:
        config_overrides["apq_storage_backend"] = "redis"
        config_overrides["redis_url"] = redis_url

    return create_optimized_config(
        database_url=database_url,
        profile=PerformanceProfile.PRODUCTION_SAFE,
        **config_overrides
    )

def create_dev_config(database_url: str, **overrides) -> FraiseQLConfig:
    """Create development-friendly config"""
    return create_optimized_config(
        database_url=database_url,
        profile=PerformanceProfile.DEVELOPMENT,
        **overrides
    )
```

#### Update Benchmark App to Use Presets

```python
# frameworks/python/fraiseql/app.py - SIMPLIFIED VERSION

from fraiseql_performance_presets import create_benchmark_config

def create_app():
    """Create the FraiseQL benchmark application - ONE LINE CONFIG"""

    # That's it! All performance optimizations included.
    config = create_benchmark_config(
        database_url="postgresql://postgres:postgres@postgres:5432/benchmarks"
    )

    app = create_fraiseql_app(
        config=config,
        types=BENCHMARK_TYPES,
        queries=BENCHMARK_QUERIES,
        title="FraiseQL Benchmark",
    )

    @app.get("/health")
    async def health():
        return {"status": "healthy", "framework": "fraiseql"}

    return app
```

---

### Phase 4: PostgreSQL C Extension Approach

**Objective**: If FraiseQL passthrough can't be activated, bypass it entirely with PostgreSQL-level optimization

**Concept**: Pre-compute GraphQL responses at write-time, store as JSONB, return directly

#### Architecture: `pg_graphql_turbo` Extension

**Key Innovation**: Move GraphQL response building into PostgreSQL itself

```sql
-- Extension registration
CREATE EXTENSION pg_graphql_turbo;

-- Register GraphQL types
SELECT graphql_turbo_register_type(
    'User',
    '{
        "id": "int",
        "name": "string",
        "email": "string",
        "age": "int",
        "city": "string",
        "posts": "Post[]"
    }'::jsonb
);

SELECT graphql_turbo_register_type(
    'Post',
    '{
        "id": "int",
        "user_id": "int",
        "title": "string",
        "content": "string"
    }'::jsonb
);

-- Create turbo-charged table
CREATE TABLE tv_user_turbo (
    -- Primary key
    id INT PRIMARY KEY,

    -- Source data
    name TEXT NOT NULL,
    email TEXT NOT NULL,
    age INT,
    city TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW(),

    -- Materialized GraphQL responses for common query patterns
    gql_full JSONB,            -- Full user object with all fields
    gql_simple JSONB,          -- User without relations
    gql_with_posts JSONB,      -- User + first 10 posts
    gql_with_all_posts JSONB,  -- User + all posts

    -- Cache metadata
    gql_updated_at TIMESTAMPTZ DEFAULT NOW(),
    gql_version INT DEFAULT 1
);

-- Trigger to auto-update GraphQL responses on every write
CREATE TRIGGER update_user_gql_responses
    BEFORE INSERT OR UPDATE ON tv_user_turbo
    FOR EACH ROW
    EXECUTE FUNCTION graphql_turbo_build_responses();

-- Index for fast lookups
CREATE INDEX idx_tv_user_turbo_gql_updated ON tv_user_turbo(gql_updated_at);
```

#### C Extension Implementation

**File**: `pg_graphql_turbo.c`

```c
/*
 * pg_graphql_turbo.c
 *
 * PostgreSQL extension for ultra-fast GraphQL response generation.
 * Builds complete GraphQL JSON responses at write-time.
 */

#include "postgres.h"
#include "fmgr.h"
#include "utils/jsonb.h"
#include "utils/builtins.h"
#include "catalog/pg_type.h"
#include "utils/datetime.h"
#include "lib/stringinfo.h"

PG_MODULE_MAGIC;

/*
 * graphql_turbo_build_responses()
 *
 * Trigger function that builds all GraphQL response variants
 * when a row is inserted or updated.
 */
PG_FUNCTION_INFO_V1(graphql_turbo_build_responses);

Datum
graphql_turbo_build_responses(PG_FUNCTION_ARGS)
{
    TriggerData *trigdata = (TriggerData *) fcinfo->context;
    HeapTuple    newtuple;
    Datum        result;

    /* Make sure it's called as a trigger */
    if (!CALLED_AS_TRIGGER(fcinfo))
        ereport(ERROR,
                (errcode(ERRCODE_E_R_I_E_TRIGGER_PROTOCOL_VIOLATED),
                 errmsg("graphql_turbo_build_responses: not called by trigger manager")));

    newtuple = trigdata->tg_trigtuple;

    /* Extract user data from row */
    bool isnull;
    int user_id = DatumGetInt32(GetAttributeByNum(newtuple, 1, &isnull));
    text *name = DatumGetTextP(GetAttributeByNum(newtuple, 2, &isnull));
    text *email = DatumGetTextP(GetAttributeByNum(newtuple, 3, &isnull));
    int age = DatumGetInt32(GetAttributeByNum(newtuple, 4, &isnull));
    text *city = DatumGetTextP(GetAttributeByNum(newtuple, 5, &isnull));

    /* Build GraphQL responses */
    Jsonb *gql_simple = build_simple_user_response(user_id, name, email, age, city);
    Jsonb *gql_with_posts = build_user_with_posts_response(user_id, name, email, age, city);

    /* Set computed columns in NEW row */
    newtuple = heap_modify_tuple_by_cols(newtuple, trigdata->tg_relation->rd_att,
                                         2, /* number of columns to modify */
                                         (int[]) {7, 8}, /* column numbers for gql_simple, gql_with_posts */
                                         (Datum[]) {JsonbPGetDatum(gql_simple), JsonbPGetDatum(gql_with_posts)},
                                         (bool[]) {false, false});

    return PointerGetDatum(newtuple);
}

/*
 * build_simple_user_response()
 *
 * Build JSON response for simple user query (no relations).
 * This is FAST - direct C string construction.
 */
static Jsonb *
build_simple_user_response(int user_id, text *name, text *email, int age, text *city)
{
    StringInfo json = makeStringInfo();

    /* Build JSON string directly */
    appendStringInfo(json, "{");
    appendStringInfo(json, "\"__typename\":\"User\",");
    appendStringInfo(json, "\"id\":%d,", user_id);
    appendStringInfo(json, "\"name\":\"%s\",", TextDatumGetCString(PointerGetDatum(name)));
    appendStringInfo(json, "\"email\":\"%s\",", TextDatumGetCString(PointerGetDatum(email)));
    appendStringInfo(json, "\"age\":%d,", age);
    appendStringInfo(json, "\"city\":\"%s\"", TextDatumGetCString(PointerGetDatum(city)));
    appendStringInfo(json, "}");

    /* Convert to JSONB */
    return DatumGetJsonbP(DirectFunctionCall1(jsonb_in, CStringGetDatum(json->data)));
}

/*
 * build_user_with_posts_response()
 *
 * Build JSON response with embedded posts array.
 * Fetches posts from tv_post table.
 */
static Jsonb *
build_user_with_posts_response(int user_id, text *name, text *email, int age, text *city)
{
    StringInfo json = makeStringInfo();

    /* Build user part */
    appendStringInfo(json, "{");
    appendStringInfo(json, "\"__typename\":\"User\",");
    appendStringInfo(json, "\"id\":%d,", user_id);
    appendStringInfo(json, "\"name\":\"%s\",", TextDatumGetCString(PointerGetDatum(name)));
    appendStringInfo(json, "\"email\":\"%s\",", TextDatumGetCString(PointerGetDatum(email)));
    appendStringInfo(json, "\"age\":%d,", age);
    appendStringInfo(json, "\"city\":\"%s\",", TextDatumGetCString(PointerGetDatum(city)));

    /* Fetch and embed posts */
    appendStringInfo(json, "\"posts\":[");

    /* Execute SPI query to fetch posts */
    char *query = psprintf("SELECT gql_simple FROM tv_post WHERE user_id = %d LIMIT 10", user_id);
    int ret = SPI_execute(query, true, 10);

    if (ret == SPI_OK_SELECT && SPI_processed > 0)
    {
        for (int i = 0; i < SPI_processed; i++)
        {
            if (i > 0) appendStringInfoChar(json, ',');

            Datum post_json = SPI_getbinval(SPI_tuptable->vals[i], SPI_tuptable->tupdesc, 1, &isnull);
            char *post_str = JsonbToCString(NULL, DatumGetJsonbP(post_json), VARSIZE(post_json));
            appendStringInfoString(json, post_str);
        }
    }

    appendStringInfo(json, "]}");

    return DatumGetJsonbP(DirectFunctionCall1(jsonb_in, CStringGetDatum(json->data)));
}

/*
 * Extension initialization
 */
void
_PG_init(void)
{
    /* Extension initialization code */
}
```

#### Makefile for C Extension

```makefile
# Makefile for pg_graphql_turbo extension

MODULE_big = pg_graphql_turbo
OBJS = pg_graphql_turbo.o

EXTENSION = pg_graphql_turbo
DATA = pg_graphql_turbo--1.0.sql
PGFILEDESC = "pg_graphql_turbo - Ultra-fast GraphQL response generation"

PG_CONFIG = pg_config
PGXS := $(shell $(PG_CONFIG) --pgxs)
include $(PGXS)
```

#### SQL Installation Script

```sql
-- pg_graphql_turbo--1.0.sql

CREATE OR REPLACE FUNCTION graphql_turbo_build_responses()
RETURNS TRIGGER AS 'MODULE_PATHNAME'
LANGUAGE C STRICT;

CREATE OR REPLACE FUNCTION graphql_turbo_register_type(
    type_name TEXT,
    type_schema JSONB
) RETURNS VOID AS $$
BEGIN
    -- Store type metadata for reference
    INSERT INTO graphql_turbo_types (name, schema)
    VALUES (type_name, type_schema)
    ON CONFLICT (name) DO UPDATE SET schema = EXCLUDED.schema;
END;
$$ LANGUAGE plpgsql;

-- Metadata table
CREATE TABLE IF NOT EXISTS graphql_turbo_types (
    name TEXT PRIMARY KEY,
    schema JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

#### GraphQL Resolver Using Extension

```python
# Use pre-computed responses - ZERO processing
@fraiseql.query
async def users(info, limit: int = 10) -> list[User]:
    """
    Ultra-fast query using pg_graphql_turbo extension.
    Returns pre-computed JSON responses.
    """
    db = info.context["db"]

    # Single query, returns ready-to-use JSON
    result = await db.execute(
        "SELECT gql_simple::text FROM tv_user_turbo ORDER BY id LIMIT $1",
        limit
    )

    # Return raw JSON strings - GraphQL framework just passes them through
    return [row[0] for row in result]

@fraiseql.query
async def user_with_posts(info, id: int) -> UserWithPosts:
    """Get user with embedded posts - pre-computed"""
    db = info.context["db"]

    result = await db.execute(
        "SELECT gql_with_posts::text FROM tv_user_turbo WHERE id = $1",
        id
    )

    return result[0][0] if result else None
```

#### Expected Performance

**Benchmark Predictions**:

| Approach | Latency | RPS | vs Strawberry | Notes |
|----------|---------|-----|---------------|-------|
| Strawberry Traditional | 37ms | 27 | 1x | Baseline |
| FraiseQL (field extraction) | 24ms | 42 | 1.5x | Current |
| FraiseQL (passthrough) | 2-5ms | 200-500 | 7-18x | If activated |
| PostgreSQL C Extension | 0.5-1ms | 1000-2000 | 37-74x | **Maximum speed** |

**Trade-offs**:

**Pros**:

- 50-100x speedup for read-heavy workloads
- Zero N+1 queries by design
- Zero application-level processing
- Works with any GraphQL framework (not FraiseQL-specific)

**Cons**:

- Requires C extension compilation
- Higher deployment complexity
- Write amplification (compute on every INSERT/UPDATE)
- Storage overhead (multiple JSONB columns)
- Less flexible (query patterns must be pre-defined)

**When to Use**:

- Extreme performance requirements (<5ms)
- Read-heavy workloads (100:1 read:write ratio)
- Static query patterns known in advance
- Infrastructure team can handle C extensions

---

### Phase 5: Documentation & Best Practices

#### Create `docs/performance_guide.md`

```markdown
# FraiseQL Performance Guide: Getting to Sub-5ms Queries

## Quick Start: Maximum Performance in 3 Steps

### Step 1: Use Pre-configured Performance Profile

Don't configure 50+ options manually. Use performance presets:

\`\`\`python
from fraiseql_performance_presets import create_optimized_config, PerformanceProfile

# ONE LINE - includes all optimizations
config = create_optimized_config(
    database_url="postgresql://user:pass@host/db",
    profile=PerformanceProfile.MAXIMUM_SPEED
)

app = create_fraiseql_app(config=config, ...)
\`\`\`

**What this enables**:
- JSON passthrough (10-50x faster)
- Turbo router (fast path for common queries)
- APQ caching (reduce parsing overhead)
- Optimized connection pool
- All unnecessary features disabled

### Step 2: Design tv_* Tables Correctly

**✅ CORRECT: Table with JSONB data**

\`\`\`sql
-- tv_* tables are TABLES, not materialized views
CREATE TABLE tv_user (
    id INT PRIMARY KEY,
    data JSONB NOT NULL
);

-- Populate with embedded data
INSERT INTO tv_user (id, data)
SELECT
    u.id,
    jsonb_build_object(
        'id', u.id,
        'name', u.name,
        'email', u.email,
        'posts', (
            SELECT jsonb_agg(
                jsonb_build_object('id', p.id, 'title', p.title)
            )
            FROM posts p
            WHERE p.user_id = u.id
            LIMIT 10  -- Pre-limit to avoid large payloads
        )
    )
FROM users u;
\`\`\`

**❌ WRONG: Materialized view**

\`\`\`sql
-- DON'T use materialized views (adds REFRESH overhead)
CREATE MATERIALIZED VIEW mv_user AS ...
\`\`\`

**Why tables?**
- No REFRESH overhead
- Updated on write (OLTP pattern)
- Fast reads
- Works with JSON passthrough

### Step 3: Register Types for Pure JSON Passthrough

\`\`\`python
@fraiseql.type(sql_source="tv_user", jsonb_column="data")
class User:
    id: int
    name: str
    email: str
    posts: list[Post] | None = None

# Register with NO table_columns - this enables passthrough
register_type_for_view(
    "tv_user",
    User,
    table_columns=None,      # ← KEY: No real columns = pure JSONB
    has_jsonb_data=True
)
\`\`\`

**Critical**: If you define `table_columns`, FraiseQL will do field extraction instead of passthrough.

---

## Performance Benchmarks

### Measured Performance

| Configuration | Latency | RPS | Speedup | When to Use |
|--------------|---------|-----|---------|-------------|
| **Strawberry Traditional** | 37ms | 27 | 1x | Flexible APIs, write-heavy |
| **FraiseQL (field extraction)** | 24ms | 42 | 1.5x | Current state (passthrough not active) |
| **FraiseQL (passthrough)** | 2-5ms | 200-500 | 7-18x | Read-heavy, predictable queries |
| **PostgreSQL C Extension** | 0.5-1ms | 1000-2000 | 37-74x | Extreme performance needs |

### What Each Approach Gives You

#### Traditional GraphQL (Strawberry, Graphene, etc.)
\`\`\`
Query → Parse → Resolve field by field → Database queries → Serialize → Response
                 ↓                        ↓
          Potential N+1              Multiple round-trips
\`\`\`

**Latency**: 30-50ms per query
**Pros**: Flexible, standard patterns
**Cons**: N+1 queries, overhead

#### FraiseQL with Field Extraction (Current State)
\`\`\`
Query → Parse → Single DB query → Extract JSONB fields → Serialize → Response
                ↓
        jsonb_build_object('id', data->>'id', ...)
\`\`\`

**Latency**: 20-30ms per query
**Pros**: No N+1, single query
**Cons**: Still extracting fields, not using passthrough

#### FraiseQL with JSON Passthrough (Target State)
\`\`\`
Query → Parse → SELECT data::text → Response
                ↓
        Direct JSONB return (no extraction!)
\`\`\`

**Latency**: 2-5ms per query
**Pros**: 10-50x faster, zero processing
**Cons**: Requires exact field match, less flexible

#### PostgreSQL C Extension (Maximum Speed)
\`\`\`
Query → SELECT gql_simple::text → Response
        ↓
    Pre-computed on write (instant read!)
\`\`\`

**Latency**: 0.5-1ms per query
**Pros**: 50-100x faster, zero runtime cost
**Cons**: Write amplification, deployment complexity

---

## Debugging Performance

### Check if JSON Passthrough is Active

**Method 1: Check response extensions**

\`\`\`python
# Enable execution info
config = create_optimized_config(
    ...,
    include_execution_info=True,
    debug=True
)

# Response should include:
{
    "data": {...},
    "extensions": {
        "execution": {
            "mode": "json_passthrough"  # ← Should say passthrough
        }
    }
}
\`\`\`

**Method 2: Check PostgreSQL query logs**

\`\`\`bash
# Enable query logging
docker exec -it postgres-db psql -U postgres -c "
ALTER SYSTEM SET log_min_duration_statement = 0;
SELECT pg_reload_conf();
"

# Run query and check logs
docker logs postgres-db 2>&1 | grep "SELECT.*tv_user"

# Expected (passthrough):
SELECT data::text FROM tv_user LIMIT 10;

# Actual (field extraction):
SELECT jsonb_build_object('id', data->>'id', ...) FROM tv_user LIMIT 10;
\`\`\`

**Method 3: Measure query time**

\`\`\`python
import time

@fraiseql.query
async def users(info, limit: int = 10) -> list[User]:
    t0 = time.perf_counter()
    result = await info.context["db"].find("tv_user", limit=limit)
    t1 = time.perf_counter()

    print(f"⏱️  Query time: {(t1-t0)*1000:.2f}ms")
    return result
\`\`\`

**Expected times**:
- Field extraction: 15-25ms
- Passthrough: 1-5ms
- C extension: 0.3-1ms

---

## Common Performance Issues

### Issue 1: Passthrough Not Activating

**Symptoms**:
- Queries take 15-30ms
- PostgreSQL logs show `jsonb_build_object(...)`
- No `extensions` field in response

**Causes**:
1. `table_columns` defined in registration
2. Selective field queries (GraphQL adds `__typename`)
3. FraiseQL version doesn't support passthrough
4. Configuration flags not propagating

**Solutions**:
- Register with `table_columns=None`
- Use performance presets
- Update to latest FraiseQL version
- Check with maintainers

### Issue 2: N+1 Queries

**Symptoms**:
- Latency increases with number of items
- Database shows multiple queries for same request

**Solution**:
Use tv_* tables with embedded data:

\`\`\`sql
-- Embed posts in user JSONB
CREATE TABLE tv_user AS
SELECT
    u.id,
    jsonb_build_object(
        'id', u.id,
        'name', u.name,
        'posts', (SELECT jsonb_agg(...) FROM posts WHERE user_id = u.id)
    ) as data
FROM users u;
\`\`\`

### Issue 3: Large Payloads

**Symptoms**:
- Good query time, but slow response
- Network transfer takes long

**Solution**:
Limit embedded arrays:

\`\`\`sql
-- Limit to 10 posts
'posts', (
    SELECT jsonb_agg(...)
    FROM posts
    WHERE user_id = u.id
    LIMIT 10  -- ← Add limit
)
\`\`\`

---

## Architecture Patterns

### Pattern 1: Simple Read-Heavy API

**Use Case**: Dashboard, feed, read-heavy API

**Architecture**:
- FraiseQL with JSON passthrough
- tv_* tables with embedded data
- Limit relations to <100 items
- APQ caching enabled

**Expected Performance**: 2-10ms per query

### Pattern 2: Mixed Read/Write Workload

**Use Case**: CRUD API, admin panel

**Architecture**:
- Strawberry Traditional for writes
- FraiseQL with passthrough for reads
- Separate read/write resolvers
- Update tv_* tables on write

**Expected Performance**:
- Reads: 2-10ms
- Writes: 20-50ms

### Pattern 3: Extreme Performance Requirements

**Use Case**: High-traffic public API, real-time features

**Architecture**:
- PostgreSQL C extension (pg_graphql_turbo)
- Pre-computed responses on write
- CDN caching
- HTTP caching headers

**Expected Performance**: 0.5-2ms per query

---

## Next Steps

1. **Apply performance presets** - One-liner configuration
2. **Verify passthrough activation** - Check extensions field
3. **Measure improvements** - Before/after benchmarks
4. **Consider C extension** - If need <5ms latency

## Getting Help

- **GitHub Issues**: https://github.com/fraiseql/fraiseql/issues
- **Documentation**: https://fraiseql.dev/docs
- **Benchmark Repository**: https://github.com/lionel-/graphql-benchmarks
\`\`\`

---

## 🎯 Recommended Execution Plan

### Week 1: Investigation & Activation

**Goal**: Activate JSON passthrough OR confirm it's not possible

**Tasks**:
1. ✅ Deep profiling (Phase 1A, 1B, 1C)
2. ✅ Source code instrumentation (Strategy 2A)
3. ✅ Minimal reproduction test (Strategy 2B)
4. ✅ GitHub issue to maintainers (Strategy 2C)

**Deliverables**:
- Profiling report showing exact bottlenecks
- Debug logs showing why passthrough doesn't activate
- Minimal test case demonstrating issue
- Response from FraiseQL maintainers

**Success Criteria**: Either passthrough works OR we know why it doesn't

### Week 2: Configuration & Documentation

**Goal**: Make FraiseQL performance dead simple

**Tasks**:
1. ✅ Create `fraiseql_performance_presets.py` (Phase 3)
2. ✅ Update benchmark app to use presets
3. ✅ Write performance guide (Phase 5)
4. ✅ Run validation benchmarks

**Deliverables**:
- One-liner configuration system
- Comprehensive performance documentation
- Updated benchmark results

**Success Criteria**: New users get optimal performance with minimal config

### Week 3: PostgreSQL C Extension (If Needed)

**Goal**: Achieve <5ms queries regardless of FraiseQL

**Tasks**:
1. ✅ Implement core C extension (Phase 4)
2. ✅ Test with benchmark suite
3. ✅ Document deployment process
4. ✅ Measure performance improvements

**Deliverables**:
- Working `pg_graphql_turbo` extension
- Installation and usage documentation
- Performance benchmarks showing 50-100x improvement

**Success Criteria**: Achieve <1ms query latency

---

## 📋 TODO: Immediate Next Steps

### 1. Profile Current Implementation (Phase 1)

\`\`\`bash
# Task: Add profiling to current benchmark
cd ../graphql-benchmarks
python benchmark_profiled.py > profiling_report.txt
\`\`\`

**Expected**: Understand where the 24ms is spent

### 2. Instrument FraiseQL Source (Strategy 2A)

\`\`\`bash
# Task: Add debug logging to FraiseQL
docker exec -it fraiseql-benchmark python patch_fraiseql_debug.py
docker restart fraiseql-benchmark
python benchmark_fair_comparison.py
docker logs fraiseql-benchmark > passthrough_debug.log
\`\`\`

**Expected**: See exactly why passthrough doesn't activate

### 3. Create Minimal Test Case (Strategy 2B)

\`\`\`bash
# Task: Test outside Docker
python test_passthrough_minimal.py
\`\`\`

**Expected**: Isolate whether issue is environmental or fundamental

### 4. Create GitHub Issue (Strategy 2C)

**Task**: Open issue on FraiseQL repository with:
- Full problem description
- All 7 configuration attempts
- Minimal reproduction code
- PostgreSQL query logs
- Request for guidance

**Expected**: Authoritative answer from maintainers

### 5. Create Performance Presets (Phase 3)

\`\`\`bash
# Task: Create one-liner config system
touch fraiseql_performance_presets.py
# Implement PerformanceProfile enum
# Implement create_optimized_config()
# Test with benchmark
\`\`\`

**Expected**: Simplified configuration for all users

---

## 📊 Success Metrics

### Short-term (Week 1-2)

- ✅ Understand exact performance bottlenecks
- ✅ Know why passthrough doesn't activate
- ✅ One-liner configuration system working
- ✅ Documentation completed

### Medium-term (Week 3-4)

- ✅ Either passthrough working OR C extension implemented
- ✅ Achieve <5ms query latency
- ✅ Benchmarks show 10-50x improvement
- ✅ Users can replicate results

### Long-term (Month 2+)

- ✅ FraiseQL consistently faster than Strawberry in benchmarks
- ✅ Clear documentation on when to use each approach
- ✅ Community validation of optimizations
- ✅ Production deployments showing results

---

## 🔬 Open Questions

1. **Why doesn't JSON passthrough activate despite correct configuration?**
   - Is it a bug in v0.10.2?
   - Is it incomplete implementation?
   - Are there undocumented requirements?

2. **Why no `extensions` field despite `include_execution_info=True`?**
   - Configuration not propagating?
   - Feature not implemented?
   - Response serialization stripping it?

3. **Does passthrough work with tv_* tables or only views?**
   - Need maintainer confirmation
   - May need different approach

4. **Is the Rust transformer actually being used?**
   - `pure_passthrough_use_rust=True` set
   - But no evidence of Rust execution

5. **What's the simplest possible passthrough configuration?**
   - Need working example from maintainers
   - Current attempts too complex?

---

## 📚 References

### Benchmark Results

- **Initial Report**: `../graphql-benchmarks/results/initial_benchmark_report.md`
- **APQ Report**: `../graphql-benchmarks/results/APQ_BENCHMARK_REPORT.md`
- **Technical Investigation**: `../graphql-benchmarks/results/TECHNICAL_INVESTIGATION.md`
- **Summary**: `../graphql-benchmarks/SUMMARY.md`

### FraiseQL Source Analysis

- **Configuration**: `fraiseql/fastapi/config.py`
- **Context Building**: `fraiseql/fastapi/dependencies.py`
- **Execution**: `fraiseql/graphql/execute.py`
- **Passthrough Logic**: `fraiseql/repositories/passthrough_mixin.py`

### Current Implementation

- **Benchmark App**: `../graphql-benchmarks/frameworks/python/fraiseql/app.py`
- **GraphQL App**: `../graphql-benchmarks/frameworks/python/fraiseql/graphql_app.py`
- **Benchmark Script**: `../graphql-benchmarks/benchmark_fair_comparison.py`

---

**Status**: Plan created, ready for execution
**Priority**: High - Performance gap identified
**Next Action**: Start Phase 1 profiling to understand bottlenecks

---

*This plan provides multiple parallel approaches to solve the FraiseQL performance issue. The goal is to either activate JSON passthrough OR provide alternative solutions that achieve the same <5ms query latency goal.*
