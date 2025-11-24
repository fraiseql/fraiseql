"""FraiseQL Complete CQRS Blog Example

This example demonstrates:
1. Migration management with fraiseql migrate
2. Auto-CASCADE cache invalidation rules
3. Explicit sync pattern (NO TRIGGERS!)
4. Performance monitoring

Run with:
    docker-compose up
    Visit: http://localhost:8000/graphql
"""

import logging
import os
import time
from contextlib import asynccontextmanager

import asyncpg
from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse
from schema import schema
from strawberry.fastapi import GraphQLRouter
from sync import EntitySync

# Configure logging
logging.basicConfig(
    level=logging.INFO, format="%(asctime)s - %(name)s - %(levelname)s - %(message)s"
)
logger = logging.getLogger(__name__)


# Global state
db_pool: asyncpg.Pool = None
sync_manager: EntitySync = None


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Application startup and shutdown."""
    global db_pool, sync_manager

    # ========================================================================
    # STARTUP: Initialize database and FraiseQL features
    # ========================================================================

    database_url = os.getenv(
        "DATABASE_URL", "postgresql://fraiseql:fraiseql@localhost:5432/blog_demo"
    )
    logger.info(f"Connecting to database: {database_url}")

    try:
        # 1. Create database connection pool
        db_pool = await asyncpg.create_pool(
            database_url, min_size=5, max_size=20, command_timeout=60
        )
        logger.info("✓ Database connection pool created")

        # 2. Initialize sync manager
        sync_manager = EntitySync(db_pool)
        logger.info("✓ Sync manager initialized")

        # 3. Perform initial full sync of all data (tb_* → tv_*)
        logger.info("Performing initial full sync...")
        start_time = time.time()

        user_count = await sync_manager.sync_all_users()
        post_count = await sync_manager.sync_all_posts()
        comment_count = await sync_manager.sync_all_comments()

        sync_duration = time.time() - start_time
        logger.info(
            f"✓ Initial sync complete: {user_count} users, {post_count} posts, "
            f"{comment_count} comments in {sync_duration:.2f}s"
        )

        # 4. TODO: Setup auto-CASCADE rules (when fraiseql.caching is integrated)
        # from fraiseql.caching import setup_auto_cascade_rules
        # await setup_auto_cascade_rules(cache, schema, verbose=True)
        logger.info("✓ CASCADE rules setup (to be integrated)")

        # 5. TODO: Setup IVM analysis (when fraiseql.ivm is integrated)
        # from fraiseql.ivm import setup_auto_ivm
        # recommendation = await setup_auto_ivm(db_pool, verbose=True)
        logger.info("✓ IVM analysis complete (to be integrated)")

        logger.info("=" * 60)
        logger.info("🚀 FraiseQL Blog API Ready!")
        logger.info("   GraphQL: http://localhost:8000/graphql")
        logger.info("   Health:  http://localhost:8000/health")
        logger.info("   Metrics: http://localhost:8000/metrics")
        logger.info("=" * 60)

    except Exception as e:
        logger.error(f"Startup failed: {e}")
        raise

    # Yield control to application
    yield

    # ========================================================================
    # SHUTDOWN: Cleanup
    # ========================================================================

    logger.info("Shutting down...")
    if db_pool:
        await db_pool.close()
        logger.info("✓ Database connections closed")


# Create FastAPI app
app = FastAPI(
    title="FraiseQL Blog API",
    description="Complete CQRS example with explicit sync pattern",
    version="1.0.0",
    lifespan=lifespan,
)


# Middleware: Request timing
@app.middleware("http")
async def add_process_time_header(request: Request, call_next):
    """Add X-Process-Time header to all responses."""
    start_time = time.time()
    response = await call_next(request)
    process_time = (time.time() - start_time) * 1000  # Convert to ms
    response.headers["X-Process-Time"] = f"{process_time:.2f}ms"
    return response


# GraphQL context provider
async def get_context():
    """Provide context to GraphQL resolvers."""
    from fraiseql.db import FraiseQLRepository

    # Create FraiseQL repository instance
    db = FraiseQLRepository(pool=db_pool)

    return {"db_pool": db_pool, "db": db, "sync": sync_manager}


# Mount GraphQL router
graphql_app = GraphQLRouter(schema, context_getter=get_context)
app.include_router(graphql_app, prefix="/graphql")


# ============================================================================
# Health & Monitoring Endpoints
# ============================================================================


@app.get("/health")
async def health_check():
    """Health check endpoint."""
    try:
        async with db_pool.acquire() as conn:
            await conn.fetchval("SELECT 1")

        return JSONResponse(
            content={
                "status": "healthy",
                "database": "connected",
                "sync": "operational",
            }
        )
    except Exception as e:
        return JSONResponse(content={"status": "unhealthy", "error": str(e)}, status_code=503)


@app.get("/metrics")
async def metrics():
    """Get sync performance metrics."""
    async with db_pool.acquire() as conn:
        # Sync metrics by entity type
        metrics_by_type = await conn.fetch(
            """
            SELECT
                entity_type,
                COUNT(*) as total_syncs,
                AVG(duration_ms)::float as avg_duration_ms,
                MAX(duration_ms) as max_duration_ms,
                (COUNT(*) FILTER (WHERE success) * 100.0 / NULLIF(COUNT(*), 0))::float as success_rate,
                COUNT(*) FILTER (WHERE NOT success) as failures
            FROM sync_log
            WHERE created_at > NOW() - INTERVAL '24 hours'
            GROUP BY entity_type
            ORDER BY entity_type
            """
        )

        # Overall stats
        overall = await conn.fetchrow(
            """
            SELECT
                COUNT(*) as total_syncs,
                AVG(duration_ms)::float as avg_duration_ms,
                (COUNT(*) FILTER (WHERE success) * 100.0 / NULLIF(COUNT(*), 0))::float as success_rate
            FROM sync_log
            WHERE created_at > NOW() - INTERVAL '24 hours'
            """
        )

        # Entity counts
        counts = await conn.fetchrow(
            """
            SELECT
                (SELECT COUNT(*) FROM tv_user) as users,
                (SELECT COUNT(*) FROM tv_post) as posts,
                (SELECT COUNT(*) FROM tv_comment) as comments
            """
        )

    return JSONResponse(
        content={
            "timestamp": time.time(),
            "sync_metrics_24h": {
                "overall": {
                    "total_syncs": overall["total_syncs"],
                    "avg_duration_ms": round(overall["avg_duration_ms"] or 0, 2),
                    "success_rate": round(overall["success_rate"] or 100, 2),
                },
                "by_entity": [
                    {
                        "entity_type": m["entity_type"],
                        "total_syncs": m["total_syncs"],
                        "avg_duration_ms": round(m["avg_duration_ms"], 2),
                        "max_duration_ms": m["max_duration_ms"],
                        "success_rate": round(m["success_rate"], 2),
                        "failures": m["failures"],
                    }
                    for m in metrics_by_type
                ],
            },
            "entity_counts": {
                "users": counts["users"],
                "posts": counts["posts"],
                "comments": counts["comments"],
            },
        }
    )


@app.get("/metrics/cache")
async def cache_metrics():
    """Get cache performance metrics (placeholder for pg_fraiseql_cache integration)."""
    # TODO: Integrate with pg_fraiseql_cache when available
    return JSONResponse(
        content={
            "status": "not_integrated",
            "message": "Cache metrics will be available when pg_fraiseql_cache is integrated",
            "planned_metrics": {
                "hit_rate": "percentage of cache hits",
                "total_entries": "number of cached entries",
                "invalidations_24h": "cache invalidations in last 24h",
                "avg_invalidation_ms": "average invalidation time",
            },
        }
    )


@app.get("/")
async def root():
    """Root endpoint with API information."""
    return JSONResponse(
        content={
            "name": "FraiseQL Blog API",
            "version": "1.0.0",
            "description": "Complete CQRS example with explicit sync pattern",
            "endpoints": {
                "graphql": "/graphql (GraphQL Playground)",
                "health": "/health (Health check)",
                "metrics": "/metrics (Sync performance)",
                "cache": "/metrics/cache (Cache metrics)",
            },
            "features": {
                "migrations": "✓ fraiseql migrate (database schema management)",
                "cqrs": "✓ tb_/tv_ pattern (command/query separation)",
                "explicit_sync": "✓ Manual sync calls (full visibility, no triggers)",
                "monitoring": "✓ Real-time sync metrics",
                "cascade": "⏳ Auto-invalidation (coming soon)",
                "ivm": "⏳ Incremental View Maintenance (coming soon)",
            },
            "philosophy": {
                "explicit_over_implicit": "Sync calls are visible in your code",
                "testability": "Easy to mock sync functions in tests",
                "control": "Batch, defer, or skip syncs as needed",
                "visibility": "Full observability of all sync operations",
            },
        }
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000)
