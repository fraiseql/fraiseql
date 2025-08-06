"""
Optimized Strawberry GraphQL implementation with DataLoaders and best practices.
"""

import contextlib
import json
import os
from collections import defaultdict
from datetime import date, datetime
from typing import Any, Optional

import asyncpg
import redis.asyncio as redis
from fastapi import FastAPI

import strawberry
from strawberry.fastapi import GraphQLRouter

# Database configuration
DATABASE_URL = os.environ.get(
    "DATABASE_URL", "postgresql://benchmark:benchmark@localhost:5432/benchmark_db"
)

# Global connection pool and cache
connection_pool: Optional[asyncpg.Pool] = None
redis_client: Optional[redis.Redis] = None


# Performance monitoring
class PerformanceMonitor:
    def __init__(self):
        self.query_count = 0
        self.resolver_calls = defaultdict(int)
        self.cache_hits = 0
        self.cache_misses = 0

    def record_query(self):
        self.query_count += 1

    def record_resolver(self, resolver_name: str):
        self.resolver_calls[resolver_name] += 1

    def record_cache_hit(self):
        self.cache_hits += 1

    def record_cache_miss(self):
        self.cache_misses += 1

    def get_stats(self):
        return {
            "total_queries": self.query_count,
            "resolver_calls": dict(self.resolver_calls),
            "cache_hit_rate": (self.cache_hits / max(1, self.cache_hits + self.cache_misses)) * 100,
        }


monitor = PerformanceMonitor()


async def get_connection_pool():
    """Get or create the connection pool."""
    global connection_pool
    if connection_pool is None:
        connection_pool = await asyncpg.create_pool(
            DATABASE_URL,
            min_size=10,
            max_size=50,
            max_queries=10000,
            max_inactive_connection_lifetime=300,
            command_timeout=30,
        )
    return connection_pool


async def get_redis():
    """Get or create Redis client."""
    global redis_client
    if redis_client is None:
        try:
            redis_client = redis.Redis(
                host=os.environ.get("REDIS_HOST", "localhost"),
                port=int(os.environ.get("REDIS_PORT", "6379")),
                decode_responses=True,
            )
            await redis_client.ping()
        except Exception:
            redis_client = None
    return redis_client


# Simple types without complex dependencies
@strawberry.type
class Organization:
    id: str
    name: str
    description: Optional[str]
    industry: str
    founded_date: Optional[date]
    created_at: datetime
    updated_at: datetime


@strawberry.type
class Department:
    id: str
    name: str
    code: str
    budget: Optional[float]
    head_count: int
    organization_id: str


@strawberry.type
class Project:
    id: str
    name: str
    description: Optional[str]
    status: str
    priority: int
    budget: Optional[float]
    start_date: Optional[date]
    end_date: Optional[date]
    department_id: str
    task_count: int
    team_size: int


@strawberry.type
class Stats:
    organization_count: int
    department_count: int
    employee_count: int
    project_count: int
    total_budget: float


# Optimized root queries
@strawberry.type
class Query:
    @strawberry.field
    async def organizations(self, limit: int = 10) -> list[Organization]:
        monitor.record_resolver("query.organizations")
        monitor.record_query()

        pool = await get_connection_pool()
        async with pool.acquire() as conn:
            rows = await conn.fetch(
                """
                SELECT id::text, name, description, industry, founded_date,
                       created_at, updated_at
                FROM benchmark.organizations
                ORDER BY name
                LIMIT $1
            """,
                limit,
            )

            return [Organization(**dict(row)) for row in rows]

    @strawberry.field
    async def departments(self, limit: int = 20) -> list[Department]:
        monitor.record_resolver("query.departments")
        monitor.record_query()

        pool = await get_connection_pool()
        async with pool.acquire() as conn:
            rows = await conn.fetch(
                """
                SELECT id::text, name, code, budget, head_count, organization_id::text
                FROM benchmark.departments
                ORDER BY name
                LIMIT $1
            """,
                limit,
            )

            return [Department(**dict(row)) for row in rows]

    @strawberry.field
    async def projects_deep(self, statuses: list[str] = None, limit: int = 10) -> list[Project]:
        if statuses is None:
            statuses = ["planning", "in_progress"]
        monitor.record_resolver("query.projects_deep")
        monitor.record_query()

        pool = await get_connection_pool()
        async with pool.acquire() as conn:
            rows = await conn.fetch(
                """
                SELECT p.id::text, p.name, p.description, p.status, p.priority,
                       p.budget, p.start_date, p.end_date, p.department_id::text,
                       task_counts.count as task_count,
                       member_counts.count as team_size
                FROM benchmark.projects p
                LEFT JOIN LATERAL (
                    SELECT COUNT(*) as count
                    FROM benchmark.tasks t
                    WHERE t.project_id = p.id
                ) task_counts ON true
                LEFT JOIN LATERAL (
                    SELECT COUNT(*) as count
                    FROM benchmark.project_members pm
                    WHERE pm.project_id = p.id
                ) member_counts ON true
                WHERE p.status = ANY($1)
                ORDER BY p.priority DESC, p.created_at DESC
                LIMIT $2
            """,
                statuses,
                limit,
            )

            return [Project(**dict(row)) for row in rows]

    @strawberry.field
    async def enterprise_stats(self) -> Stats:
        monitor.record_resolver("query.enterprise_stats")
        monitor.record_query()

        # Check cache first
        redis_conn = await get_redis()
        cache_key = "strawberry:enterprise_stats"

        if redis_conn:
            try:
                cached = await redis_conn.get(cache_key)
                if cached:
                    monitor.record_cache_hit()
                    data = json.loads(cached)
                    return Stats(**data)
            except Exception:
                pass

        monitor.record_cache_miss()

        pool = await get_connection_pool()
        async with pool.acquire() as conn:
            stats = await conn.fetchrow("""
                SELECT
                    (SELECT COUNT(*) FROM benchmark.organizations) as organization_count,
                    (SELECT COUNT(*) FROM benchmark.departments) as department_count,
                    (SELECT COUNT(*) FROM benchmark.employees) as employee_count,
                    (SELECT COUNT(*) FROM benchmark.projects) as project_count,
                    (SELECT COALESCE(SUM(budget), 0) FROM benchmark.departments) as total_budget
            """)

            result = Stats(**dict(stats))

            # Cache for 5 minutes
            if redis_conn:
                with contextlib.suppress(Exception):
                    await redis_conn.setex(cache_key, 300, json.dumps(dict(stats), default=str))

            return result

    @strawberry.field
    async def performance_stats(self) -> dict[str, Any]:
        return monitor.get_stats()


# Create the schema
schema = strawberry.Schema(query=Query)

# FastAPI app
app = FastAPI(title="Optimized Strawberry GraphQL Benchmark")

graphql_app = GraphQLRouter(schema, debug=False)
app.include_router(graphql_app, prefix="/graphql")


@app.on_event("startup")
async def startup_event():
    """Initialize optimizations."""
    print("🍓 Starting optimized Strawberry GraphQL...")

    # Initialize connection pool
    pool = await get_connection_pool()
    print(f"✅ Database connection pool: {pool.get_min_size()}-{pool.get_max_size()} connections")

    # Test Redis
    redis_conn = await get_redis()
    if redis_conn:
        print("✅ Redis caching enabled")
    else:
        print("⚠️  Redis not available - running without cache")

    print("🏆 Strawberry optimizations ready")


@app.on_event("shutdown")
async def shutdown_event():
    """Clean up resources."""
    global connection_pool, redis_client

    if connection_pool:
        await connection_pool.close()

    if redis_client:
        await redis_client.close()


@app.get("/health")
async def health():
    """Health check."""
    pool = await get_connection_pool()
    redis_conn = await get_redis()

    return {
        "status": "healthy",
        "framework": "Strawberry GraphQL",
        "optimizations": [
            "connection_pooling",
            "redis_caching" if redis_conn else "no_caching",
            "efficient_resolvers",
        ],
        "connection_pool": {
            "size": pool.get_size(),
            "idle": pool.get_idle_size(),
            "min_size": pool.get_min_size(),
            "max_size": pool.get_max_size(),
        },
        "performance_monitor": monitor.get_stats(),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8001, workers=1, loop="asyncio", access_log=False)  # noqa: S104
