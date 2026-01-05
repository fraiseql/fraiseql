# Performance Tuning: Starlette

**Version**: 2.0.0+
**Reading Time**: 35 minutes
**Audience**: DevOps engineers, backend developers
**Prerequisites**: Completed [Production Deployment](./03-deployment.md)

---

## Overview

This guide covers optimizing your Starlette GraphQL server for high performance:
- ✅ Worker and process optimization
- ✅ Connection pool tuning
- ✅ Query optimization
- ✅ Caching strategies
- ✅ Async I/O patterns
- ✅ Benchmarking techniques
- ✅ Common bottlenecks and fixes

---

## Measuring Current Performance

### Baseline Benchmarks

Before optimizing, establish baselines:

```bash
# Install apache bench
pip install apache2-utils

# Baseline: 100 concurrent requests, 1000 total
ab -n 1000 -c 100 http://localhost:8000/graphql

# Result: Requests per second, response times
# Example output:
# Requests per second:    1000.0 [#/sec] (mean)
# Time per request:       10.0ms (mean, across all concurrent requests)
```

### Locust Load Testing

```python
# locustfile.py
from locust import HttpUser, task

class GraphQLUser(HttpUser):
    @task
    def graphql_query(self):
        self.client.post(
            "/graphql",
            json={
                "query": "{ users { id name } }",
                "variables": {},
            },
            headers={"Content-Type": "application/json"}
        )

# Run load test
# locust -f locustfile.py --host http://localhost:8000
```

### Python Profiling

```python
# profile_app.py
import cProfile
import pstats
from main import app
import uvicorn

if __name__ == "__main__":
    profiler = cProfile.Profile()
    profiler.enable()

    # Run for a while
    config = uvicorn.Config(app, host="0.0.0.0", port=8000)
    server = uvicorn.Server(config)

    # After some time, disable and print stats
    profiler.disable()
    stats = pstats.Stats(profiler)
    stats.sort_stats("cumulative")
    stats.print_stats(20)
```

---

## Worker Configuration

### Gunicorn Worker Settings

**Optimize worker count**:
```bash
# Formula: (2 * number_of_cores) + 1
# For 4-core server: (2 * 4) + 1 = 9 workers

gunicorn \
  --workers 9 \
  --worker-class uvicorn.workers.UvicornWorker \
  --bind 0.0.0.0:8000 \
  --timeout 120 \
  --max-requests 1000 \
  --max-requests-jitter 100 \
  main:app
```

### Worker Monitoring

```bash
# Check worker status
ps aux | grep gunicorn

# Monitor in real-time
watch -n 1 'ps aux | grep gunicorn | wc -l'
```

### Graceful Reload

```bash
# Reload workers without dropping requests
kill -HUP <gunicorn-pid>

# Check if reload succeeded
ps aux | grep gunicorn
```

---

## Connection Pool Tuning

### Database Connection Pool

**Optimal sizing**:
```python
from sqlalchemy import create_engine
from sqlalchemy.pool import QueuePool

# Formula: (workers * 5) is usually good starting point
# For 9 workers: 9 * 5 = 45 connections

engine = create_engine(
    DATABASE_URL,
    poolclass=QueuePool,
    pool_size=20,              # Connections per worker
    max_overflow=10,           # Extra connections when needed
    pool_recycle=3600,         # Recycle connections after 1 hour
    echo_pool=True,            # Log pool events (development only)
)
```

### Connection Pool Monitoring

```python
from sqlalchemy import event

@event.listens_for(engine, "connect")
def receive_connect(dbapi_conn, connection_record):
    """Log connection events"""
    pool = engine.pool
    print(f"Pool size: {pool.size()}, Checked out: {pool.checkedout()}")

@event.listens_for(engine, "close")
def receive_close(dbapi_conn, connection_record):
    """Log close events"""
    pool = engine.pool
    print(f"Closing connection, pool size: {pool.size()}")
```

---

## Query Optimization

### N+1 Query Problem

**Identify N+1 patterns**:
```python
# ❌ N+1 Problem - 1 query for users + N queries for posts
users = await db.execute("SELECT * FROM users")
for user in users:
    posts = await db.execute(
        "SELECT * FROM posts WHERE user_id = %s",
        [user.id]
    )
    user.posts = posts

# ✅ Solution - 2 queries total using JOIN
query = """
SELECT users.*, posts.*
FROM users
LEFT JOIN posts ON users.id = posts.user_id
"""
results = await db.execute(query)

# Or use batch loading
user_ids = [u.id for u in users]
posts = await db.execute(
    "SELECT * FROM posts WHERE user_id = ANY(%s)",
    [user_ids]
)
```

### Query Analysis

```bash
# Enable query logging
SQLALCHEMY_ECHO=true python main.py

# Look for:
# - Queries taking > 100ms
# - Repeated identical queries
# - Missing indexes
```

### Eager Loading

```python
from sqlalchemy.orm import selectinload

# ❌ Lazy loading (N+1 problem)
users = await session.execute(select(User))

# ✅ Eager loading (all at once)
users = await session.execute(
    select(User).options(selectinload(User.posts))
)
```

---

## Caching Strategies

### In-Memory Caching

```python
from functools import lru_cache
import asyncio

# Simple cache decorator
@lru_cache(maxsize=1000)
def expensive_computation(x: int) -> int:
    return x ** 2

# Async cache with TTL
from datetime import datetime, timedelta

class AsyncCache:
    def __init__(self, ttl_seconds: int = 300):
        self.cache = {}
        self.ttl = ttl_seconds

    async def get(self, key: str):
        if key in self.cache:
            value, created_at = self.cache[key]
            if datetime.now() - created_at < timedelta(seconds=self.ttl):
                return value
            else:
                del self.cache[key]
        return None

    async def set(self, key: str, value):
        self.cache[key] = (value, datetime.now())

# Use in handler
cache = AsyncCache(ttl_seconds=300)

async def graphql_handler(request: Request):
    query = str(request.query_params)

    # Check cache first
    cached = await cache.get(query)
    if cached:
        return JSONResponse(cached)

    # Execute query
    result = await schema.execute(...)

    # Store in cache
    await cache.set(query, result)

    return JSONResponse(result)
```

### Redis Caching

```python
import aioredis
from starlette.requests import Request
from starlette.responses import JSONResponse

# Create Redis connection
redis = await aioredis.create_redis_pool('redis://localhost')

async def graphql_handler(request: Request):
    data = await request.json()
    query = data.get("query")

    # Create cache key from query
    cache_key = f"graphql:{hash(query)}"

    # Try to get from cache
    cached = await redis.get(cache_key)
    if cached:
        return JSONResponse(json.loads(cached))

    # Execute query
    result = await schema.execute(query)

    # Store in cache (5 minute TTL)
    await redis.setex(
        cache_key,
        300,
        json.dumps(result)
    )

    return JSONResponse(result)
```

### Cache Invalidation

```python
class CacheManager:
    def __init__(self, redis):
        self.redis = redis
        self.tags = {}  # tag -> cached_keys

    async def get(self, key: str):
        return await self.redis.get(key)

    async def set(self, key: str, value, tags: list = None, ttl: int = 300):
        await self.redis.setex(key, ttl, value)

        if tags:
            for tag in tags:
                if tag not in self.tags:
                    self.tags[tag] = []
                self.tags[tag].append(key)

    async def invalidate_tag(self, tag: str):
        """Invalidate all keys with this tag"""
        if tag in self.tags:
            for key in self.tags[tag]:
                await self.redis.delete(key)
            del self.tags[tag]

# Usage
cache = CacheManager(redis)

# Store with tags
await cache.set(
    "user:123",
    user_data,
    tags=["users", "active"],
    ttl=300
)

# Invalidate all user caches
await cache.invalidate_tag("users")
```

---

## Async I/O Optimization

### Proper Async Patterns

```python
import asyncio

# ❌ Bad - blocks the event loop
def slow_operation():
    import time
    time.sleep(5)  # Blocks event loop!
    return result

# ✅ Good - truly async
async def fast_operation():
    await asyncio.sleep(5)  # Non-blocking
    return result

# ✅ Run CPU-intensive work in thread pool
def cpu_intensive():
    return sum(range(1000000))

async def handler(request: Request):
    loop = asyncio.get_event_loop()
    result = await loop.run_in_executor(None, cpu_intensive)
    return JSONResponse({"result": result})
```

### Concurrent Requests

```python
import asyncio

async def graphql_handler(request: Request):
    data = await request.json()

    # If handling batch requests
    queries = data.get("queries", [])

    # Execute all concurrently
    results = await asyncio.gather(*[
        schema.execute(q) for q in queries
    ])

    return JSONResponse([r.data for r in results])
```

---

## Database Optimization

### Connection Reuse

```python
from starlette.requests import Request

# Store connection pool in app state
@app.on_event("startup")
async def startup():
    app.state.db_pool = await create_pool()

@app.on_event("shutdown")
async def shutdown():
    await app.state.db_pool.close()

# Use in handlers
async def handler(request: Request):
    pool = request.app.state.db_pool
    async with pool.acquire() as conn:
        result = await conn.fetch("SELECT * FROM users")
    return JSONResponse(result)
```

### Bulk Operations

```python
# ❌ Slow - individual INSERTs
for user in users:
    await db.execute(
        "INSERT INTO users (name, email) VALUES (%s, %s)",
        [user.name, user.email]
    )

# ✅ Fast - bulk INSERT
await db.executemany(
    "INSERT INTO users (name, email) VALUES (%s, %s)",
    [(u.name, u.email) for u in users]
)
```

---

## Memory Optimization

### Monitor Memory Usage

```python
import psutil
import os

def get_memory_usage():
    process = psutil.Process(os.getpid())
    return process.memory_info().rss / 1024 / 1024  # MB

# Log periodically
import logging
import asyncio

async def monitor_memory():
    while True:
        mem = get_memory_usage()
        if mem > 500:  # Alert if > 500MB
            logging.warning(f"High memory usage: {mem:.2f}MB")
        await asyncio.sleep(60)

# Start in background
@app.on_event("startup")
async def startup():
    asyncio.create_task(monitor_memory())
```

### Reduce Allocations

```python
# ❌ Allocates new list each time
async def handler(request: Request):
    users = []
    for i in range(1000):
        users.append({"id": i, "name": f"user{i}"})
    return JSONResponse(users)

# ✅ Pre-allocate with appropriate size
async def handler(request: Request):
    users = [None] * 1000  # Pre-allocate
    for i in range(1000):
        users[i] = {"id": i, "name": f"user{i}"}
    return JSONResponse(users)
```

---

## Common Performance Issues

### Issue 1: Slow Requests

**Diagnosis**:
```bash
# Enable query logging
SQLALCHEMY_ECHO=true uvicorn main:app

# Check request timing
curl -w "Total: %{time_total}s\n" http://localhost:8000/graphql
```

**Common causes**:

1. **Missing database indexes**
```sql
-- Add index for common queries
CREATE INDEX idx_users_id ON users(id);
CREATE INDEX idx_posts_user_id ON posts(user_id);
```

2. **Inefficient GraphQL query**
```graphql
# ❌ Fetches everything
{ users { id name email posts { id title comments { id text } } } }

# ✅ Fetch only what's needed
{ users { id name } }
```

### Issue 2: High CPU Usage

**Diagnosis**:
```bash
# Use py-spy for profiling
pip install py-spy
py-spy record -o profile.svg -- python main.py

# Look for hot functions
```

**Solutions**:
1. Cache expensive computations
2. Optimize algorithms (O(n²) → O(n))
3. Use async operations for I/O

### Issue 3: Memory Leaks

**Diagnosis**:
```python
# Monitor memory over time
import tracemalloc

tracemalloc.start()

# ... run your code ...

current, peak = tracemalloc.get_traced_memory()
print(f"Current: {current / 1024 / 1024:.2f}MB")
print(f"Peak: {peak / 1024 / 1024:.2f}MB")
```

**Solutions**:
1. Use bounded caches with TTL
2. Close connections properly
3. Avoid circular references

---

## Performance Checklist

Before deploying:

- [ ] Baselines established (ab, locust)
- [ ] Worker count optimized
- [ ] Connection pool sized appropriately
- [ ] Database queries optimized (no N+1)
- [ ] Caching strategy implemented
- [ ] Async patterns used correctly
- [ ] Memory usage stable
- [ ] Logging configured for production
- [ ] Monitoring in place
- [ ] Load tested (1000+ req/s)

---

## Next Steps

- **Seeing slowness in production?** → [Troubleshooting](./05-troubleshooting.md)
- **Back to Deployment?** → [Production Deployment](./03-deployment.md)
- **Getting started?** → [Getting Started](./01-getting-started.md)

---

**You're now optimized for scale!** Monitor metrics and adjust based on real-world performance. 🚀
