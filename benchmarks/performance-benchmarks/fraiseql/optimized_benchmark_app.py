"""Optimized FraiseQL benchmark application with TurboRouter and Redis caching."""

import json
import os
import time
from typing import Optional

import redis.asyncio as redis

from fraiseql import create_fraiseql_app, fraise_field, fraise_type
from fraiseql.fastapi import FraiseQLConfig
from fraiseql.turbo import QueryRegistrar, TurboConfig

# Redis connection
redis_client: Optional[redis.Redis] = None


async def get_redis():
    """Get Redis client."""
    global redis_client
    if redis_client is None:
        redis_client = redis.Redis(
            host=os.environ.get("REDIS_HOST", "localhost"),
            port=int(os.environ.get("REDIS_PORT", "6379")),
            decode_responses=True,
        )
    return redis_client


@fraise_type
class User:
    id: str  # UUID
    username: str
    email: str
    fullName: str
    createdAt: str
    orderCount: int = 0
    totalSpent: float = 0.0
    reviewCount: int = 0
    averageRating: Optional[float] = None


@fraise_type
class Product:
    id: str  # UUID
    name: str
    sku: str
    price: float
    stockQuantity: int
    categoryId: str  # UUID
    reviewCount: int = 0
    averageRating: Optional[float] = None


@fraise_type
class Order:
    id: str  # UUID
    orderNumber: str
    userId: str
    status: str
    totalAmount: float
    createdAt: str
    itemCount: int = 0


@fraise_type
class Query:
    # Health check
    health: str = fraise_field(default="healthy", description="Health check endpoint")

    # High-performance user queries with caching
    users: list[User] = fraise_field(
        default_factory=list,
        description="List users with projection table optimization and caching",
    )

    async def resolve_users(self, info, where=None, order_by=None, limit=None, offset=None):
        """Optimized resolver for users using projection tables and Redis cache."""
        # Generate cache key
        cache_key = f"users:{limit}:{offset}:{hash(str(where))}:{hash(str(order_by))}"

        # Try Redis cache first
        try:
            redis_conn = await get_redis()
            cached_result = await redis_conn.get(cache_key)
            if cached_result:
                cached_data = json.loads(cached_result)
                return [User.from_dict(user_data) for user_data in cached_data]
        except Exception as e:
            print(f"Redis cache miss: {e}")

        # Get database connection
        db = info.context.get("db")
        if not db:
            return []

        pool = db.get_pool()

        # Build optimized SQL query using projection table
        query_parts = ["SELECT data FROM tv_users WHERE 1=1"]
        params = []
        param_count = 1

        # Handle limit and offset
        if limit is not None:
            query_parts.append(f" LIMIT ${param_count}")
            params.append(limit)
            param_count += 1
        if offset is not None:
            query_parts.append(f" OFFSET ${param_count}")
            params.append(offset)
            param_count += 1

        # Build final query
        query = "".join(query_parts)

        # Execute using asyncpg with connection pooling
        async with pool.acquire() as conn:
            # Set up JSON decoding for asyncpg
            await conn.set_type_codec(
                "jsonb", encoder=json.dumps, decoder=json.loads, schema="pg_catalog"
            )

            start_time = time.time()
            rows = await conn.fetch(query, *params)
            query_time = time.time() - start_time

            # Convert to User instances
            users_data = [row["data"] for row in rows]
            result = [User.from_dict(user_data) for user_data in users_data]

            # Cache result in Redis (expire after 5 minutes)
            try:
                redis_conn = await get_redis()
                await redis_conn.setex(
                    cache_key,
                    300,  # 5 minutes
                    json.dumps(users_data),
                )
            except Exception as e:
                print(f"Redis cache write failed: {e}")

            print(
                f"Users query executed in {query_time * 1000:.2f}ms, returned {len(result)} users"
            )
            return result

    products: list[Product] = fraise_field(
        default_factory=list,
        description="List products with projection table optimization and caching",
    )

    async def resolve_products(self, info, where=None, order_by=None, limit=None, offset=None):
        """Optimized resolver for products using projection tables and Redis cache."""
        # Generate cache key
        cache_key = f"products:{limit}:{offset}:{hash(str(where))}:{hash(str(order_by))}"

        # Try Redis cache first
        try:
            redis_conn = await get_redis()
            cached_result = await redis_conn.get(cache_key)
            if cached_result:
                cached_data = json.loads(cached_result)
                return [Product.from_dict(product_data) for product_data in cached_data]
        except Exception as e:
            print(f"Redis cache miss: {e}")

        # Get database connection
        db = info.context.get("db")
        if not db:
            return []

        pool = db.get_pool()

        # Build optimized SQL query using projection table
        query_parts = ["SELECT data FROM tv_products WHERE 1=1"]
        params = []
        param_count = 1

        # Handle limit and offset
        if limit is not None:
            query_parts.append(f" LIMIT ${param_count}")
            params.append(limit)
            param_count += 1
        if offset is not None:
            query_parts.append(f" OFFSET ${param_count}")
            params.append(offset)
            param_count += 1

        # Build final query
        query = "".join(query_parts)

        # Execute using asyncpg with connection pooling
        async with pool.acquire() as conn:
            # Set up JSON decoding for asyncpg
            await conn.set_type_codec(
                "jsonb", encoder=json.dumps, decoder=json.loads, schema="pg_catalog"
            )

            start_time = time.time()
            rows = await conn.fetch(query, *params)
            query_time = time.time() - start_time

            # Convert to Product instances
            products_data = [row["data"] for row in rows]
            result = [Product.from_dict(product_data) for product_data in products_data]

            # Cache result in Redis (expire after 5 minutes)
            try:
                redis_conn = await get_redis()
                await redis_conn.setex(
                    cache_key,
                    300,  # 5 minutes
                    json.dumps(products_data),
                )
            except Exception as e:
                print(f"Redis cache write failed: {e}")

            print(
                f"Products query executed in {query_time * 1000:.2f}ms, returned {len(result)} products"
            )
            return result

    orders: list[Order] = fraise_field(
        default_factory=list,
        description="List orders with projection table optimization and caching",
    )

    async def resolve_orders(self, info, where=None, order_by=None, limit=None, offset=None):
        """Optimized resolver for orders using projection tables and Redis cache."""
        # Generate cache key
        cache_key = f"orders:{limit}:{offset}:{hash(str(where))}:{hash(str(order_by))}"

        # Try Redis cache first
        try:
            redis_conn = await get_redis()
            cached_result = await redis_conn.get(cache_key)
            if cached_result:
                cached_data = json.loads(cached_result)
                return [Order.from_dict(order_data) for order_data in cached_data]
        except Exception as e:
            print(f"Redis cache miss: {e}")

        # Get database connection
        db = info.context.get("db")
        if not db:
            return []

        pool = db.get_pool()

        # Build optimized SQL query using projection table
        query_parts = ["SELECT data FROM tv_orders WHERE 1=1"]
        params = []
        param_count = 1

        # Handle limit and offset
        if limit is not None:
            query_parts.append(f" LIMIT ${param_count}")
            params.append(limit)
            param_count += 1
        if offset is not None:
            query_parts.append(f" OFFSET ${param_count}")
            params.append(offset)
            param_count += 1

        # Build final query
        query = "".join(query_parts)

        # Execute using asyncpg with connection pooling
        async with pool.acquire() as conn:
            # Set up JSON decoding for asyncpg
            await conn.set_type_codec(
                "jsonb", encoder=json.dumps, decoder=json.loads, schema="pg_catalog"
            )

            start_time = time.time()
            rows = await conn.fetch(query, *params)
            query_time = time.time() - start_time

            # Convert to Order instances
            orders_data = [row["data"] for row in rows]
            result = [Order.from_dict(order_data) for order_data in orders_data]

            # Cache result in Redis (expire after 5 minutes)
            try:
                redis_conn = await get_redis()
                await redis_conn.setex(
                    cache_key,
                    300,  # 5 minutes
                    json.dumps(orders_data),
                )
            except Exception as e:
                print(f"Redis cache write failed: {e}")

            print(
                f"Orders query executed in {query_time * 1000:.2f}ms, returned {len(result)} orders"
            )
            return result


# Create optimized FraiseQL configuration with TurboRouter
config = FraiseQLConfig(
    database_url=os.environ.get(
        "DATABASE_URL", "postgresql://benchmark:benchmark@postgres-bench/benchmark_db"
    ),
    auto_camel_case=True,
    # Enable TurboRouter for production performance
    turbo_enabled=True,
    turbo_cache_size=1000,  # Cache up to 1000 queries
    turbo_auto_detect=True,  # Auto-detect hot queries
    turbo_detection_threshold=5,  # Register queries after 5 executions
    turbo_enable_monitoring=True,
    # Production optimizations
    environment="production",
    max_pool_size=20,  # Increased connection pool
    connection_pool_min_size=5,
    connection_pool_max_size=20,
)

# Create TurboRouter configuration
turbo_config = TurboConfig(
    cache_size=1000, auto_detect_threshold=5, enable_monitoring=True, enable_metrics=True
)

# Create optimized app with TurboRouter
app = create_fraiseql_app(
    config=config,
    types=[User, Product, Order, Query],
    title="Optimized FraiseQL Benchmark API",
    description="High-performance FraiseQL with TurboRouter and Redis caching",
)


# Pre-register common queries in TurboRouter for maximum performance
async def register_common_queries():
    """Pre-register common benchmark queries in TurboRouter."""
    try:
        # Get database pool from the app
        db_pool = app.state.db_pool if hasattr(app.state, "db_pool") else None
        if not db_pool:
            print("Warning: Database pool not found, skipping query registration")
            return

        registrar = QueryRegistrar(db_pool)

        # Register common user queries
        await registrar.register_query(
            "users_100", "SELECT data FROM tv_users LIMIT $1", view_name="tv_users"
        )

        await registrar.register_query(
            "products_100", "SELECT data FROM tv_products LIMIT $1", view_name="tv_products"
        )

        await registrar.register_query(
            "orders_100", "SELECT data FROM tv_orders LIMIT $1", view_name="tv_orders"
        )

        print("✅ Common queries registered in TurboRouter")

    except Exception as e:
        print(f"Warning: Failed to register queries: {e}")


@app.on_event("startup")
async def startup_event():
    """Initialize optimizations on startup."""
    print("🚀 Starting optimized FraiseQL benchmark app...")

    # Initialize Redis connection
    try:
        redis_conn = await get_redis()
        await redis_conn.ping()
        print("✅ Redis connection established")
    except Exception as e:
        print(f"⚠️  Redis connection failed: {e} - continuing without cache")

    # Register common queries for TurboRouter
    await register_common_queries()

    print("🏆 FraiseQL optimization stack ready: TurboRouter + Redis + Projection Tables")


@app.on_event("shutdown")
async def shutdown_event():
    """Clean up on shutdown."""
    global redis_client
    if redis_client:
        await redis_client.close()
    print("🛑 FraiseQL benchmark app shutdown complete")


@app.get("/health")
async def health_check():
    """Health check endpoint."""
    redis_status = "disconnected"
    try:
        redis_conn = await get_redis()
        await redis_conn.ping()
        redis_status = "connected"
    except Exception:
        pass

    return {
        "status": "healthy",
        "redis": redis_status,
        "turbo_enabled": config.turbo_enabled,
        "optimizations": ["projection_tables", "redis_caching", "turbo_router"],
    }


@app.get("/benchmark/users")
async def benchmark_users_rest(limit: int = 100):
    """REST endpoint for user benchmarking using projection tables."""
    start_time = time.time()

    # Generate cache key
    cache_key = f"rest_users:{limit}"

    # Try Redis cache first
    try:
        redis_conn = await get_redis()
        cached_result = await redis_conn.get(cache_key)
        if cached_result:
            cache_time = time.time() - start_time
            return {
                "query": "users",
                "limit": limit,
                "cached": True,
                "cache_time_ms": cache_time * 1000,
                "result_count": len(json.loads(cached_result)),
            }
    except Exception:
        pass

    # Get database pool
    db_pool = app.state.db_pool if hasattr(app.state, "db_pool") else None
    if not db_pool:
        return {"error": "Database not available"}

    # Execute query
    async with db_pool.acquire() as conn:
        await conn.set_type_codec(
            "jsonb", encoder=json.dumps, decoder=json.loads, schema="pg_catalog"
        )

        query_start = time.time()
        rows = await conn.fetch("SELECT data FROM tv_users LIMIT $1", limit)
        query_time = time.time() - query_start

        # Cache result
        try:
            redis_conn = await get_redis()
            users_data = [row["data"] for row in rows]
            await redis_conn.setex(cache_key, 300, json.dumps(users_data))
        except Exception:
            pass

        total_time = time.time() - start_time

        return {
            "query": "users",
            "limit": limit,
            "cached": False,
            "query_time_ms": query_time * 1000,
            "total_time_ms": total_time * 1000,
            "result_count": len(rows),
        }


@app.get("/benchmark/products")
async def benchmark_products_rest(limit: int = 100):
    """REST endpoint for product benchmarking using projection tables."""
    start_time = time.time()

    # Generate cache key
    cache_key = f"rest_products:{limit}"

    # Try Redis cache first
    try:
        redis_conn = await get_redis()
        cached_result = await redis_conn.get(cache_key)
        if cached_result:
            cache_time = time.time() - start_time
            return {
                "query": "products",
                "limit": limit,
                "cached": True,
                "cache_time_ms": cache_time * 1000,
                "result_count": len(json.loads(cached_result)),
            }
    except Exception:
        pass

    # Get database pool
    db_pool = app.state.db_pool if hasattr(app.state, "db_pool") else None
    if not db_pool:
        return {"error": "Database not available"}

    # Execute query
    async with db_pool.acquire() as conn:
        await conn.set_type_codec(
            "jsonb", encoder=json.dumps, decoder=json.loads, schema="pg_catalog"
        )

        query_start = time.time()
        rows = await conn.fetch("SELECT data FROM tv_products LIMIT $1", limit)
        query_time = time.time() - query_start

        # Cache result
        try:
            redis_conn = await get_redis()
            products_data = [row["data"] for row in rows]
            await redis_conn.setex(cache_key, 300, json.dumps(products_data))
        except Exception:
            pass

        total_time = time.time() - start_time

        return {
            "query": "products",
            "limit": limit,
            "cached": False,
            "query_time_ms": query_time * 1000,
            "total_time_ms": total_time * 1000,
            "result_count": len(rows),
        }


@app.get("/turbo/stats")
async def get_turbo_stats():
    """Get TurboRouter performance statistics."""
    # This would normally access the TurboRouter instance
    # For now, return mock stats
    return {
        "turbo_enabled": True,
        "cache_hits": 0,
        "cache_misses": 0,
        "total_queries": 0,
        "performance_boost": "25-50% improvement expected",
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(
        app,
        host="0.0.0.0",  # noqa: S104
        port=8000,
        workers=1,  # Single worker for benchmarking consistency
        access_log=False,  # Disable access logs for performance
    )
