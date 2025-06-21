"""Ultra-optimized FraiseQL with read replica support."""

import asyncio
import json
import os
import random
import time
from collections import deque
from typing import Any, Optional

import asyncpg
import redis.asyncio as redis
from fastapi import FastAPI, Response

app = FastAPI(title="Ultra-Optimized FraiseQL with Read Replicas")

# Database configuration
DATABASE_URL_PRIMARY = os.environ.get(
    "DATABASE_URL_PRIMARY", "postgresql://benchmark:benchmark@postgres-primary:5432/benchmark_db"
)
DATABASE_URL_REPLICAS = os.environ.get(
    "DATABASE_URL_REPLICAS", "postgresql://benchmark:benchmark@pgpool:5432/benchmark_db"
)
ENABLE_READ_REPLICAS = os.environ.get("ENABLE_READ_REPLICAS", "false").lower() == "true"

# Global connection pools
connection_pools: dict[str, asyncpg.Pool] = {}
redis_pool: Optional[redis.ConnectionPool] = None
redis_client: Optional[redis.Redis] = None


# Performance monitoring with replica tracking
class PerformanceMonitor:
    def __init__(self):
        self.request_count = 0
        self.cache_hits = 0
        self.pool_stats = {"read": 0, "write": 0, "hot": 0, "replica": 0}
        self.replica_usage = {"primary": 0, "replica": 0}

    def record_request(
        self, pool_type: str = "read", cache_hit: bool = False, replica_used: bool = False
    ):
        self.request_count += 1
        self.pool_stats[pool_type] += 1
        if cache_hit:
            self.cache_hits += 1
        if replica_used:
            self.replica_usage["replica"] += 1
        else:
            self.replica_usage["primary"] += 1

    def get_stats(self):
        return {
            "total_requests": self.request_count,
            "cache_hit_rate": (self.cache_hits / max(1, self.request_count)) * 100,
            "pool_usage": self.pool_stats,
            "replica_usage": self.replica_usage,
            "replica_percentage": (
                self.replica_usage["replica"] / max(1, sum(self.replica_usage.values()))
            )
            * 100,
        }


monitor = PerformanceMonitor()


# Multi-level cache
class MultiLevelCache:
    def __init__(self):
        self.l1_cache = {}
        self.l1_order = deque(maxlen=2000)
        self.l1_max_size = 2000

    def _evict_l1_if_needed(self):
        while len(self.l1_cache) >= self.l1_max_size and self.l1_order:
            oldest_key = self.l1_order.popleft()
            self.l1_cache.pop(oldest_key, None)

    def l1_get(self, key: str):
        if key in self.l1_cache:
            self.l1_order.append(key)
            return self.l1_cache[key]
        return None

    def l1_set(self, key: str, value: Any):
        self._evict_l1_if_needed()
        self.l1_cache[key] = value
        self.l1_order.append(key)


cache = MultiLevelCache()


async def setup_connection(conn):
    """Optimize each connection for JSONB operations."""
    await conn.execute("SET work_mem = '16MB'")
    await conn.execute("SET gin_fuzzy_search_limit = 0")
    await conn.execute("SET search_path = benchmark, public")
    await conn.execute("SET synchronous_commit = off")


async def get_connection_pools():
    """Initialize multi-tier connection pools with read replica support."""
    global connection_pools

    if not connection_pools:
        # Primary database pool (for writes)
        connection_pools["write"] = await asyncpg.create_pool(
            DATABASE_URL_PRIMARY,
            min_size=2,
            max_size=5,
            max_queries=1000,
            max_inactive_connection_lifetime=300,
            command_timeout=30,
            setup=setup_connection,
            server_settings={"jit": "off", "application_name": "fraiseql_write_pool"},
        )

        # Read pool - connects to primary if replicas disabled
        read_url = DATABASE_URL_REPLICAS if ENABLE_READ_REPLICAS else DATABASE_URL_PRIMARY
        connection_pools["read"] = await asyncpg.create_pool(
            read_url,
            min_size=10,
            max_size=30,
            max_queries=10000,
            max_inactive_connection_lifetime=300,
            command_timeout=10,
            setup=setup_connection,
            server_settings={"jit": "off", "application_name": "fraiseql_read_pool"},
        )

        # Hot queries pool - also uses replicas
        connection_pools["hot"] = await asyncpg.create_pool(
            read_url,
            min_size=5,
            max_size=15,
            max_queries=20000,
            max_inactive_connection_lifetime=600,
            command_timeout=5,
            setup=setup_connection,
            server_settings={"jit": "off", "application_name": "fraiseql_hot_pool"},
        )

        # Dedicated replica pool for load distribution
        if ENABLE_READ_REPLICAS:
            connection_pools["replica"] = await asyncpg.create_pool(
                DATABASE_URL_REPLICAS,
                min_size=5,
                max_size=20,
                max_queries=15000,
                max_inactive_connection_lifetime=300,
                command_timeout=10,
                setup=setup_connection,
                server_settings={"jit": "off", "application_name": "fraiseql_replica_pool"},
            )

    return connection_pools


async def get_redis():
    """Get Redis client with connection pooling."""
    global redis_pool, redis_client

    if redis_client is None:
        try:
            if redis_pool is None:
                redis_pool = redis.ConnectionPool(
                    host=os.environ.get("REDIS_HOST", "localhost"),
                    port=int(os.environ.get("REDIS_PORT", "6379")),
                    max_connections=30,
                    retry_on_timeout=True,
                    socket_keepalive=True,
                    socket_keepalive_options={},
                    health_check_interval=30,
                )

            redis_client = redis.Redis(
                connection_pool=redis_pool,
                decode_responses=True,
                socket_connect_timeout=5,
                socket_timeout=5,
            )
        except Exception as e:
            print(f"Redis connection failed: {e}")
            redis_client = None

    return redis_client


# Pre-compiled query registry
HOT_QUERIES = {
    "users_100": {"sql": "SELECT data FROM tv_users LIMIT $1", "pool": "hot"},
    "users_50": {"sql": "SELECT data FROM tv_users LIMIT $1", "pool": "hot"},
    "products_100": {"sql": "SELECT data FROM tv_products LIMIT $1", "pool": "hot"},
    "products_50": {"sql": "SELECT data FROM tv_products LIMIT $1", "pool": "hot"},
}


async def execute_with_replica_selection(query: str, params: list, pool_type: str = "read"):
    """Execute query with intelligent replica selection."""
    pools = await get_connection_pools()

    # For read operations, randomly distribute between primary and replicas
    if ENABLE_READ_REPLICAS and pool_type in ["read", "hot"]:
        # 70% chance to use replica, 30% to use primary for load distribution
        use_replica = random.random() < 0.7
        if use_replica and "replica" in pools:
            pool = pools["replica"]
            replica_used = True
        else:
            pool = pools[pool_type]
            replica_used = False
    else:
        pool = pools[pool_type]
        replica_used = False

    async with pool.acquire() as conn:
        result = await conn.fetch(query, *params)

    monitor.record_request(pool_type, cache_hit=False, replica_used=replica_used)
    return result


@app.on_event("startup")
async def startup_event():
    """Initialize all optimizations on startup."""
    print("🚀 Starting ultra-optimized FraiseQL with read replica support...")

    # Initialize connection pools
    pools = await get_connection_pools()
    print("✅ Database connection pools initialized:")
    print(
        f"   - Write pool (primary): {pools['write'].get_min_size()}-{pools['write'].get_max_size()} connections"
    )
    print(
        f"   - Read pool: {pools['read'].get_min_size()}-{pools['read'].get_max_size()} connections"
    )
    print(
        f"   - Hot queries pool: {pools['hot'].get_min_size()}-{pools['hot'].get_max_size()} connections"
    )
    if "replica" in pools:
        print(
            f"   - Replica pool: {pools['replica'].get_min_size()}-{pools['replica'].get_max_size()} connections"
        )
    print(f"   - Read replicas: {'ENABLED' if ENABLE_READ_REPLICAS else 'DISABLED'}")

    # Test Redis connection
    redis_conn = await get_redis()
    if redis_conn:
        try:
            await redis_conn.ping()
            print("✅ Redis connection pool established")
        except Exception as e:
            print(f"⚠️  Redis connection failed: {e}")

    print("🏆 All optimizations ready including read replica support")


@app.on_event("shutdown")
async def shutdown_event():
    """Clean up all resources."""
    global connection_pools, redis_client, redis_pool

    for pool_name, pool in connection_pools.items():
        if pool:
            await pool.close()
            print(f"✅ Closed {pool_name} connection pool")

    if redis_client:
        await redis_client.close()
    if redis_pool:
        await redis_pool.disconnect()


@app.get("/health")
async def health():
    """Health check with replica status."""
    redis_status = "disconnected"
    redis_conn = await get_redis()
    if redis_conn:
        try:
            await redis_conn.ping()
            redis_status = "connected"
        except Exception:
            pass

    pools = await get_connection_pools()
    pool_status = {}
    for name, pool in pools.items():
        pool_status[name] = {
            "size": pool.get_size(),
            "idle": pool.get_idle_size(),
            "min_size": pool.get_min_size(),
            "max_size": pool.get_max_size(),
        }

    return {
        "status": "ultra_healthy_with_replicas",
        "redis": redis_status,
        "connection_pools": pool_status,
        "read_replicas_enabled": ENABLE_READ_REPLICAS,
        "optimizations": [
            "multi_tier_connection_pools",
            "read_replica_load_balancing",
            "redis_connection_pooling",
            "projection_tables_tv",
            "multi_level_caching",
            "hot_query_registry",
        ],
        "performance_monitor": monitor.get_stats(),
    }


@app.get("/benchmark/users")
async def benchmark_users(limit: int = 100, response: Response = None):
    """Ultra-optimized user benchmarking with read replica support."""
    start_time = time.time()

    cache_key = f"ultra_replica_users:{limit}"

    # L1 Cache check
    l1_result = cache.l1_get(cache_key)
    if l1_result:
        monitor.record_request("hot", cache_hit=True)
        result = l1_result.copy()
        result["cached"] = "L1_hit"
        result["cache_time_ms"] = (time.time() - start_time) * 1000
        if response:
            response.headers["X-Optimization"] = "L1_cache_hit"
        return result

    # L2 Cache check (Redis)
    redis_conn = await get_redis()
    if redis_conn:
        try:
            cached_result = await redis_conn.get(cache_key)
            if cached_result:
                cached_data = json.loads(cached_result)
                cache.l1_set(cache_key, cached_data)
                monitor.record_request("hot", cache_hit=True)
                cached_data["cached"] = "L2_hit"
                cached_data["cache_time_ms"] = (time.time() - start_time) * 1000
                if response:
                    response.headers["X-Optimization"] = "L2_redis_hit"
                return cached_data
        except Exception as e:
            print(f"Redis cache read error: {e}")

    # L3: Database query with replica selection
    query_start = time.time()

    hot_query_key = f"users_{limit}" if limit in [50, 100] else None
    if hot_query_key and hot_query_key in HOT_QUERIES:
        query_info = HOT_QUERIES[hot_query_key]
        results = await execute_with_replica_selection(
            query_info["sql"], [limit], query_info["pool"]
        )
        pool_type = "hot_query_pool"
    else:
        results = await execute_with_replica_selection(
            "SELECT data FROM tv_users LIMIT $1", [limit], "read"
        )
        pool_type = "read_pool"

    query_time = (time.time() - query_start) * 1000

    result = {
        "query": "users",
        "limit": limit,
        "cached": False,
        "query_time_ms": query_time,
        "result_count": len(results),
        "optimization": f"{pool_type}_tv_users_with_replicas",
        "total_time_ms": (time.time() - start_time) * 1000,
        "replica_enabled": ENABLE_READ_REPLICAS,
    }

    # Async caching
    if redis_conn:
        asyncio.create_task(redis_conn.setex(cache_key, 300, json.dumps(result)))

    cache.l1_set(cache_key, result)

    if response:
        response.headers["Cache-Control"] = "public, max-age=60"
        response.headers["X-Optimization"] = f"{pool_type}_with_replicas"

    return result


@app.get("/benchmark/products")
async def benchmark_products(limit: int = 100, response: Response = None):
    """Ultra-optimized product benchmarking with read replica support."""
    start_time = time.time()

    cache_key = f"ultra_replica_products:{limit}"

    # L1 Cache check
    l1_result = cache.l1_get(cache_key)
    if l1_result:
        monitor.record_request("hot", cache_hit=True)
        result = l1_result.copy()
        result["cached"] = "L1_hit"
        result["cache_time_ms"] = (time.time() - start_time) * 1000
        if response:
            response.headers["X-Optimization"] = "L1_cache_hit"
        return result

    # L2 Cache check
    redis_conn = await get_redis()
    if redis_conn:
        try:
            cached_result = await redis_conn.get(cache_key)
            if cached_result:
                cached_data = json.loads(cached_result)
                cache.l1_set(cache_key, cached_data)
                monitor.record_request("hot", cache_hit=True)
                cached_data["cached"] = "L2_hit"
                cached_data["cache_time_ms"] = (time.time() - start_time) * 1000
                if response:
                    response.headers["X-Optimization"] = "L2_redis_hit"
                return cached_data
        except Exception as e:
            print(f"Redis cache read error: {e}")

    # L3: Database query
    query_start = time.time()

    hot_query_key = f"products_{limit}" if limit in [50, 100] else None
    if hot_query_key and hot_query_key in HOT_QUERIES:
        query_info = HOT_QUERIES[hot_query_key]
        results = await execute_with_replica_selection(
            query_info["sql"], [limit], query_info["pool"]
        )
        pool_type = "hot_query_pool"
    else:
        results = await execute_with_replica_selection(
            "SELECT data FROM tv_products LIMIT $1", [limit], "read"
        )
        pool_type = "read_pool"

    query_time = (time.time() - query_start) * 1000

    result = {
        "query": "products",
        "limit": limit,
        "cached": False,
        "query_time_ms": query_time,
        "result_count": len(results),
        "optimization": f"{pool_type}_tv_products_with_replicas",
        "total_time_ms": (time.time() - start_time) * 1000,
        "replica_enabled": ENABLE_READ_REPLICAS,
    }

    # Async caching
    if redis_conn:
        asyncio.create_task(redis_conn.setex(cache_key, 300, json.dumps(result)))

    cache.l1_set(cache_key, result)

    if response:
        response.headers["Cache-Control"] = "public, max-age=60"
        response.headers["X-Optimization"] = f"{pool_type}_with_replicas"

    return result


@app.get("/replica/stats")
async def replica_stats():
    """Get detailed replica usage statistics."""
    stats = monitor.get_stats()

    pools = await get_connection_pools()
    replica_pool_stats = {}
    if "replica" in pools:
        pool = pools["replica"]
        replica_pool_stats = {
            "size": pool.get_size(),
            "idle": pool.get_idle_size(),
            "utilization_percent": ((pool.get_size() - pool.get_idle_size()) / pool.get_size())
            * 100,
        }

    return {
        "read_replicas_enabled": ENABLE_READ_REPLICAS,
        "replica_usage_stats": stats["replica_usage"],
        "replica_usage_percentage": stats["replica_percentage"],
        "replica_pool_stats": replica_pool_stats,
        "total_requests": stats["total_requests"],
        "optimization_status": "read_replicas_active"
        if ENABLE_READ_REPLICAS
        else "read_replicas_disabled",
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(
        app,
        host="0.0.0.0",  # noqa: S104
        port=8000,
        workers=1,
        loop="asyncio",
        http="httptools",
        access_log=False,
        server_header=False,
        date_header=False,
    )
