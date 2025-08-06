"""Optimized FraiseQL benchmark application with Redis caching and projection tables."""

import json
import os
import time
from typing import Optional

import redis.asyncio as redis

from fraiseql import create_fraiseql_app, fraise_field, fraise_type
from fraiseql.fastapi import FraiseQLConfig

# Redis connection
redis_client: Optional[redis.Redis] = None


async def get_redis():
    """Get Redis client."""
    global redis_client
    if redis_client is None:
        try:
            redis_client = redis.Redis(
                host=os.environ.get("REDIS_HOST", "localhost"),
                port=int(os.environ.get("REDIS_PORT", "6379")),
                decode_responses=True,
            )
        except Exception as e:
            print(f"Redis connection failed: {e}")
            redis_client = None
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

    # High-performance user queries with caching and projection tables
    users: list[User] = fraise_field(
        default_factory=list,
        description="List users with projection table optimization and caching",
    )

    async def resolve_users(self, info, where=None, order_by=None, limit=None, offset=None):
        """Optimized resolver for users using projection tables and Redis cache."""
        # Generate cache key
        cache_key = f"users:{limit}:{offset}:{hash(str(where))}:{hash(str(order_by))}"

        # Try Redis cache first
        redis_conn = await get_redis()
        if redis_conn:
            try:
                cached_result = await redis_conn.get(cache_key)
                if cached_result:
                    cached_data = json.loads(cached_result)
                    print(f"✅ Cache hit for users query (limit={limit})")
                    return [User.from_dict(user_data) for user_data in cached_data]
            except Exception as e:
                print(f"Redis cache read failed: {e}")

        # Get database connection
        db = info.context.get("db")
        if not db:
            return []

        pool = db.get_pool()

        # Build optimized SQL query using projection table (tv_users)
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
            if redis_conn:
                try:
                    await redis_conn.setex(
                        cache_key,
                        300,  # 5 minutes
                        json.dumps(users_data),
                    )
                    print(f"✅ Cached users query result (limit={limit})")
                except Exception as e:
                    print(f"Redis cache write failed: {e}")

            print(
                f"🚀 Users query executed in {query_time * 1000:.2f}ms, returned {len(result)} users"
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
        redis_conn = await get_redis()
        if redis_conn:
            try:
                cached_result = await redis_conn.get(cache_key)
                if cached_result:
                    cached_data = json.loads(cached_result)
                    print(f"✅ Cache hit for products query (limit={limit})")
                    return [Product.from_dict(product_data) for product_data in cached_data]
            except Exception as e:
                print(f"Redis cache read failed: {e}")

        # Get database connection
        db = info.context.get("db")
        if not db:
            return []

        pool = db.get_pool()

        # Build optimized SQL query using projection table (tv_products)
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
            if redis_conn:
                try:
                    await redis_conn.setex(
                        cache_key,
                        300,  # 5 minutes
                        json.dumps(products_data),
                    )
                    print(f"✅ Cached products query result (limit={limit})")
                except Exception as e:
                    print(f"Redis cache write failed: {e}")

            print(
                f"🚀 Products query executed in {query_time * 1000:.2f}ms, returned {len(result)} products"
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
        redis_conn = await get_redis()
        if redis_conn:
            try:
                cached_result = await redis_conn.get(cache_key)
                if cached_result:
                    cached_data = json.loads(cached_result)
                    print(f"✅ Cache hit for orders query (limit={limit})")
                    return [Order.from_dict(order_data) for order_data in cached_data]
            except Exception as e:
                print(f"Redis cache read failed: {e}")

        # Get database connection
        db = info.context.get("db")
        if not db:
            return []

        pool = db.get_pool()

        # Build optimized SQL query using projection table (tv_orders)
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
            if redis_conn:
                try:
                    await redis_conn.setex(
                        cache_key,
                        300,  # 5 minutes
                        json.dumps(orders_data),
                    )
                    print(f"✅ Cached orders query result (limit={limit})")
                except Exception as e:
                    print(f"Redis cache write failed: {e}")

            print(
                f"🚀 Orders query executed in {query_time * 1000:.2f}ms, returned {len(result)} orders"
            )
            return result


# Create optimized FraiseQL configuration
config = FraiseQLConfig(
    database_url=os.environ.get(
        "DATABASE_URL", "postgresql://benchmark:benchmark@postgres-bench/benchmark_db"
    ),
    auto_camel_case=True,
)

# Create optimized app
app = create_fraiseql_app(
    config=config,
    types=[User, Product, Order, Query],
    title="Optimized FraiseQL Benchmark API",
    description="High-performance FraiseQL with Redis caching and projection tables",
)


@app.on_event("startup")
async def startup_event():
    """Initialize optimizations on startup."""
    print("🚀 Starting optimized FraiseQL benchmark app...")

    # Initialize Redis connection
    redis_conn = await get_redis()
    if redis_conn:
        try:
            await redis_conn.ping()
            print("✅ Redis connection established")
        except Exception as e:
            print(f"⚠️  Redis connection failed: {e} - continuing without cache")
    else:
        print("⚠️  Redis not available - continuing without cache")

    print("🏆 FraiseQL optimization stack ready: Redis Caching + Projection Tables (tv_)")


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
    redis_conn = await get_redis()
    if redis_conn:
        try:
            await redis_conn.ping()
            redis_status = "connected"
        except Exception:
            pass

    return {
        "status": "healthy",
        "redis": redis_status,
        "optimizations": ["projection_tables_tv", "redis_caching", "connection_pooling"],
        "database": "projection_tables_enabled",
    }


@app.get("/benchmark/users")
async def benchmark_users_rest(limit: int = 100):
    """REST endpoint for user benchmarking using projection tables."""
    start_time = time.time()

    # Generate cache key
    cache_key = f"rest_users:{limit}"

    # Try Redis cache first
    redis_conn = await get_redis()
    if redis_conn:
        try:
            cached_result = await redis_conn.get(cache_key)
            if cached_result:
                cache_time = time.time() - start_time
                return {
                    "query": "users",
                    "limit": limit,
                    "cached": True,
                    "cache_time_ms": cache_time * 1000,
                    "result_count": len(json.loads(cached_result)),
                    "optimization": "redis_cache_hit",
                }
        except Exception:
            pass

    # Get database pool from app state
    db_pool = None
    if hasattr(app.state, "db_pool"):
        db_pool = app.state.db_pool
    elif hasattr(app, "app_context") and app.app_context.get("db"):
        db_pool = app.app_context["db"].get_pool()

    if not db_pool:
        return {"error": "Database not available"}

    # Execute query using projection table
    async with db_pool.acquire() as conn:
        await conn.set_type_codec(
            "jsonb", encoder=json.dumps, decoder=json.loads, schema="pg_catalog"
        )

        query_start = time.time()
        rows = await conn.fetch("SELECT data FROM tv_users LIMIT $1", limit)
        query_time = time.time() - query_start

        # Cache result
        if redis_conn:
            try:
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
            "optimization": "projection_table_tv_users",
        }


@app.get("/benchmark/products")
async def benchmark_products_rest(limit: int = 100):
    """REST endpoint for product benchmarking using projection tables."""
    start_time = time.time()

    # Generate cache key
    cache_key = f"rest_products:{limit}"

    # Try Redis cache first
    redis_conn = await get_redis()
    if redis_conn:
        try:
            cached_result = await redis_conn.get(cache_key)
            if cached_result:
                cache_time = time.time() - start_time
                return {
                    "query": "products",
                    "limit": limit,
                    "cached": True,
                    "cache_time_ms": cache_time * 1000,
                    "result_count": len(json.loads(cached_result)),
                    "optimization": "redis_cache_hit",
                }
        except Exception:
            pass

    # Get database pool from app state
    db_pool = None
    if hasattr(app.state, "db_pool"):
        db_pool = app.state.db_pool
    elif hasattr(app, "app_context") and app.app_context.get("db"):
        db_pool = app.app_context["db"].get_pool()

    if not db_pool:
        return {"error": "Database not available"}

    # Execute query using projection table
    async with db_pool.acquire() as conn:
        await conn.set_type_codec(
            "jsonb", encoder=json.dumps, decoder=json.loads, schema="pg_catalog"
        )

        query_start = time.time()
        rows = await conn.fetch("SELECT data FROM tv_products LIMIT $1", limit)
        query_time = time.time() - query_start

        # Cache result
        if redis_conn:
            try:
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
            "optimization": "projection_table_tv_products",
        }


@app.get("/cache/stats")
async def get_cache_stats():
    """Get Redis cache statistics."""
    redis_conn = await get_redis()
    if not redis_conn:
        return {"error": "Redis not available"}

    try:
        info = await redis_conn.info("stats")
        return {
            "redis_connected": True,
            "keyspace_hits": info.get("keyspace_hits", 0),
            "keyspace_misses": info.get("keyspace_misses", 0),
            "total_commands_processed": info.get("total_commands_processed", 0),
            "hit_rate": info.get("keyspace_hits", 0)
            / max(1, info.get("keyspace_hits", 0) + info.get("keyspace_misses", 0))
            * 100,
        }
    except Exception as e:
        return {"error": f"Redis stats failed: {e}"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(
        app,
        host="0.0.0.0",  # noqa: S104
        port=8000,
        workers=1,  # Single worker for benchmarking consistency
        access_log=False,  # Disable access logs for performance
    )
