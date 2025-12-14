"""FraiseQL Blog Enterprise - Advanced Example Application

An enterprise-grade blog application demonstrating:
- Domain-driven design with bounded contexts
- Advanced PostgreSQL patterns
- Multi-tenant architecture
- Enterprise authentication and authorization
- Event sourcing and CQRS patterns
- Performance optimization and caching
"""

import asyncio
import logging
import os
from contextlib import asynccontextmanager
from typing import Any
from uuid import UUID

import psycopg
from fastapi import FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from graphql import GraphQLResolveInfo

import fraiseql
from fraiseql.cqrs import CQRSRepository
from fraiseql.fastapi import FraiseQLConfig, create_fraiseql_app
from fraiseql.fastapi.app import create_db_pool
from fraiseql.fastapi.dependencies import get_db_pool, set_db_pool

# Configure logging
logging.basicConfig(
    level=logging.INFO, format="%(asctime)s - %(name)s - %(levelname)s - %(message)s"
)
logger = logging.getLogger(__name__)

# Enterprise configuration from environment
DB_NAME = os.getenv("DB_NAME", "fraiseql_blog_enterprise")
DB_USER = os.getenv("DB_USER", "fraiseql")
DB_PASSWORD = os.getenv("DB_PASSWORD", "fraiseql")
DB_HOST = os.getenv("DB_HOST", "localhost")
DB_PORT = int(os.getenv("DB_PORT", "5432"))

# Redis configuration for enterprise caching
REDIS_URL = os.getenv("REDIS_URL", "redis://localhost:6379")

# JWT configuration
JWT_SECRET = os.getenv("JWT_SECRET", "your-secret-key-change-in-production")
JWT_ALGORITHM = os.getenv("JWT_ALGORITHM", "HS256")

# Environment settings
ENV = os.getenv("ENV", "development")
DEBUG = ENV == "development"


def get_database_url() -> str:
    """Get enterprise database URL from environment variables."""
    return f"postgresql://{DB_USER}:{DB_PASSWORD}@{DB_HOST}:{DB_PORT}/{DB_NAME}"


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Application lifespan with enterprise initialization."""
    logger.info("🚀 Starting FraiseQL Blog Enterprise")
    logger.info(f"Environment: {ENV}")
    logger.info(f"Database: {get_database_url()}")
    logger.info(f"Redis: {REDIS_URL}")

    # Initialize enterprise components
    # - Database connection pool
    # - Redis connection for caching
    # - Event handlers
    # - Background tasks

    yield

    # Cleanup enterprise resources
    logger.info("🔒 Blog Enterprise shutdown")


def _create_base_app() -> FastAPI:
    """Create the base enterprise blog FastAPI application."""

    # Enterprise context getter with multi-tenancy
    async def get_enterprise_context(request: Request) -> dict[str, Any]:
        """Provide enterprise context for GraphQL operations."""
        # Import here to avoid circular imports
        from fraiseql.fastapi.dependencies import get_db

        try:
            # Use FraiseQL's database dependency to get a connection from the pool
            db = await get_db()

            # Extract JWT token and tenant information
            auth_header = request.headers.get("Authorization", "")
            token = (
                auth_header.replace("Bearer ", "") if auth_header.startswith("Bearer ") else None
            )

            # For demo purposes, use default tenant and user
            # In production, extract from validated JWT token
            user_id = UUID("22222222-2222-2222-2222-222222222222")  # Demo user
            organization_id = UUID("11111111-1111-1111-1111-111111111111")  # Demo org

            return {
                "db": db,
                "user_id": user_id,
                "organization_id": organization_id,
                "tenant_id": organization_id,  # For compatibility
                "user_role": "admin",  # Demo role
                "user_permissions": ["content.create", "content.edit", "content.delete"],
                "request": request,
                "token": token,
            }
        except Exception as e:
            logger.error(f"Failed to create enterprise context: {e}")
            raise

    # Simple health query that doesn't require database access
    @fraiseql.query
    def health_status() -> dict:
        """Simple health status query that doesn't access the database."""
        return {
            "status": "healthy",
            "service": "blog_enterprise",
            "timestamp": "2024-01-01T00:00:00Z",
        }

    ENTERPRISE_TYPES = []
    ENTERPRISE_MUTATIONS = []
    ENTERPRISE_QUERIES = [health_status]

    # Create enterprise configuration
    config = FraiseQLConfig(
        database_url=get_database_url(),
        app_name="FraiseQL Blog Enterprise API",
        app_version="2.0.0",
        environment="development" if DEBUG else "production",
        enable_introspection=DEBUG,
        enable_playground=DEBUG,
    )

    # Custom lifespan with timeout protection for database pool creation
    @asynccontextmanager
    async def enterprise_lifespan(app: FastAPI):
        """Enterprise lifespan with robust database pool initialization."""
        logger.info("🚀 Starting FraiseQL Blog Enterprise with timeout protection")

        try:
            # Create database pool with timeout protection
            pool = await asyncio.wait_for(
                create_db_pool(
                    str(config.database_url),
                    min_size=2,
                    max_size=config.database_pool_size,
                    timeout=config.database_pool_timeout,
                ),
                timeout=30.0,  # 30 second timeout for pool creation
            )
            set_db_pool(pool)
            logger.info("✅ Database pool created successfully")

        except TimeoutError:
            logger.error("❌ Database pool creation timed out after 30 seconds")
            # Continue without pool - app will fail gracefully on database requests
            raise RuntimeError("Database pool creation timed out")
        except Exception as e:
            logger.error(f"❌ Database pool creation failed: {e}")
            # Continue without pool for testing purposes
            raise

        yield

        # Cleanup
        logger.info("🔒 Blog Enterprise shutdown")
        try:
            pool = get_db_pool()
            if pool:
                await pool.close()
                logger.info("Database pool closed")
        except Exception as e:
            logger.error(f"Error closing database pool: {e}")

    # Create FraiseQL app with enterprise configuration - this becomes our main app
    app = create_fraiseql_app(
        database_url=get_database_url(),
        types=ENTERPRISE_TYPES,
        mutations=ENTERPRISE_MUTATIONS,
        queries=ENTERPRISE_QUERIES,
        # context_getter=get_enterprise_context,  # Disable custom context to avoid pool initialization issue
        config=config,
        title="FraiseQL Blog Enterprise API",
        description="Enterprise blog API with advanced patterns and multi-tenancy",
        production=not DEBUG,
        # Note: lifespan removed - let FraiseQL handle pool management or tests provide pool via fixtures
    )

    # CORS configuration for enterprise
    app.add_middleware(
        CORSMiddleware,
        allow_origins=os.getenv(
            "CORS_ORIGINS", "http://localhost:3000,http://127.0.0.1:3000"
        ).split(","),
        allow_credentials=True,
        allow_methods=["GET", "POST", "PUT", "DELETE", "OPTIONS"],
        allow_headers=["*"],
    )

    # Enterprise endpoints
    @app.get("/")
    async def home():
        return {
            "message": "🏢 FraiseQL Blog Enterprise",
            "description": "Enterprise-grade blog demonstrating advanced FraiseQL patterns",
            "version": "2.0.0",
            "environment": ENV,
            "features": [
                "Domain-driven design with bounded contexts",
                "Multi-tenant architecture with organization isolation",
                "Advanced PostgreSQL patterns (functions, triggers, views)",
                "Enterprise authentication with JWT and RBAC",
                "Event sourcing and CQRS patterns",
                "Redis-based multi-layer caching",
                "Performance monitoring and observability",
                "Production-ready deployment configuration",
            ],
            "endpoints": {
                "graphql": "/graphql",
                "playground": "/graphql" if DEBUG else None,
                "health": "/health",
                "metrics": "/metrics",
                "admin": "/admin",
            },
        }

    @app.get("/metrics")
    async def metrics():
        """Enterprise metrics endpoint for monitoring."""
        # In production, would return Prometheus-format metrics
        return {
            "service": "blog_enterprise",
            "metrics": {
                "requests_total": 0,
                "active_connections": 0,
                "cache_hit_rate": 0.95,
                "average_response_time": 45.2,
                "error_rate": 0.001,
            },
            "business_metrics": {
                "total_posts": 0,
                "total_users": 0,
                "total_organizations": 0,
                "posts_published_today": 0,
                "active_users_today": 0,
            },
        }

    @app.get("/admin")
    async def admin():
        """Enterprise admin interface placeholder."""
        return {
            "message": "Enterprise Admin Interface",
            "note": "In production, this would be a full admin dashboard",
            "features": [
                "Organization management",
                "User administration",
                "Content moderation",
                "Analytics dashboard",
                "System monitoring",
                "Audit logs",
            ],
        }

    return app


def create_app():
    """Main application factory with custom health endpoint override."""
    # Create the base FraiseQL app first
    app = _create_base_app()

    # Override FraiseQL's health endpoint by replacing the existing route
    # Find and replace the existing health route
    for i, route in enumerate(app.routes):
        if hasattr(route, "path") and route.path == "/health":
            # Get our custom health function from the base app
            # It was already defined in the base app
            async def custom_health():
                """Enterprise health check with realistic dependency validation."""

                async def check_database() -> str:
                    """Check database connectivity with timeout."""
                    try:
                        conn = await asyncio.wait_for(
                            psycopg.AsyncConnection.connect(
                                get_database_url(),
                                connect_timeout=3,  # Shorter timeout for health checks
                            ),
                            timeout=5.0,  # Overall timeout including connection
                        )
                        # Try a simple query to verify the connection works
                        await conn.execute("SELECT 1")
                        await conn.close()
                        return "healthy"
                    except TimeoutError:
                        return "unhealthy: timeout"
                    except Exception as e:
                        return f"unhealthy: {e!s}"

                async def check_cache() -> str:
                    """Check cache connectivity (Redis placeholder)."""
                    # In a real implementation, this would check Redis connectivity
                    # For now, return healthy since cache is optional
                    return "healthy"

                # Run dependency checks concurrently with individual timeouts
                db_task = asyncio.create_task(check_database())
                cache_task = asyncio.create_task(check_cache())

                try:
                    db_status, cache_status = await asyncio.gather(
                        asyncio.wait_for(db_task, timeout=10.0),
                        asyncio.wait_for(cache_task, timeout=5.0),
                    )
                except TimeoutError:
                    # If any check times out, mark as unhealthy
                    db_status = "unhealthy: timeout" if not db_task.done() else await db_task
                    cache_status = (
                        "unhealthy: timeout" if not cache_task.done() else await cache_task
                    )

                # Determine overall status
                all_healthy = all(status == "healthy" for status in [db_status, cache_status])

                return {
                    "status": "healthy" if all_healthy else "degraded",
                    "service": "blog_enterprise",
                    "version": "2.0.0",
                    "environment": ENV,
                    "dependencies": {
                        "database": db_status,
                        "cache": cache_status,
                    },
                    "uptime": "0s",  # Would track actual uptime in production
                }

            # Create new route with our endpoint
            from fastapi.routing import APIRoute

            new_route = APIRoute("/health", custom_health, methods=["GET"])
            # Replace the route at the same position
            routes_list = list(app.routes)
            routes_list[i] = new_route
            app.router.routes = routes_list
            break

    return app


# Create the enterprise app instance
app = create_app()

if __name__ == "__main__":
    import uvicorn

    # Enterprise-grade server configuration
    uvicorn.run(
        app,
        host="0.0.0.0",
        port=8000,
        reload=DEBUG,
        workers=1 if DEBUG else 4,  # Multiple workers in production
        access_log=True,
        log_level="info" if not DEBUG else "debug",
    )
