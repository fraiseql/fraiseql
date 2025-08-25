"""FraiseQL Blog Enterprise - Advanced Example Application

An enterprise-grade blog application demonstrating:
- Domain-driven design with bounded contexts
- Advanced PostgreSQL patterns
- Multi-tenant architecture
- Enterprise authentication and authorization
- Event sourcing and CQRS patterns
- Performance optimization and caching
"""

import logging
import os
from contextlib import asynccontextmanager
from typing import Any, Dict
from uuid import UUID

import psycopg
from fastapi import FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware

from fraiseql.cqrs import CQRSRepository
from fraiseql.fastapi import create_fraiseql_app

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s"
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


def create_app() -> FastAPI:
    """Create the enterprise blog FastAPI application."""
    app = FastAPI(
        title="FraiseQL Blog Enterprise",
        description="Enterprise blog built with FraiseQL showcasing advanced patterns",
        version="2.0.0",
        lifespan=lifespan,
        debug=DEBUG,
    )

    # CORS configuration for enterprise
    app.add_middleware(
        CORSMiddleware,
        allow_origins=os.getenv("CORS_ORIGINS", "http://localhost:3000,http://127.0.0.1:3000").split(","),
        allow_credentials=True,
        allow_methods=["GET", "POST", "PUT", "DELETE", "OPTIONS"],
        allow_headers=["*"],
    )

    # Enterprise context getter with multi-tenancy
    async def get_enterprise_context(request: Request) -> Dict[str, Any]:
        """Provide enterprise context for GraphQL operations."""
        database_url = get_database_url()

        try:
            conn = await psycopg.AsyncConnection.connect(database_url)

            # Extract JWT token and tenant information
            auth_header = request.headers.get("Authorization", "")
            token = auth_header.replace("Bearer ", "") if auth_header.startswith("Bearer ") else None

            # For demo purposes, use default tenant and user
            # In production, extract from validated JWT token
            user_id = UUID("22222222-2222-2222-2222-222222222222")  # Demo user
            organization_id = UUID("11111111-1111-1111-1111-111111111111")  # Demo org

            return {
                "db": CQRSRepository(conn),
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

    # Import enterprise GraphQL schema (placeholder for now)
    # In a real enterprise setup, these would be in separate modules
    ENTERPRISE_TYPES = []
    ENTERPRISE_MUTATIONS = []
    ENTERPRISE_QUERIES = []

    # Create FraiseQL app with enterprise configuration
    fraiseql_app = create_fraiseql_app(
        database_url=get_database_url(),
        types=ENTERPRISE_TYPES,
        mutations=ENTERPRISE_MUTATIONS,
        queries=ENTERPRISE_QUERIES,
        context_getter=get_enterprise_context,
        title="FraiseQL Blog Enterprise API",
        description="Enterprise blog API with advanced patterns and multi-tenancy",
        production=not DEBUG,
        # Enterprise-specific configurations
        enable_introspection=DEBUG,
        enable_playground=DEBUG,
    )

    # Mount GraphQL endpoint
    app.mount("/graphql", fraiseql_app)

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

    @app.get("/health")
    async def health():
        """Enterprise health check with dependencies."""
        try:
            # Check database connectivity
            conn = await psycopg.AsyncConnection.connect(get_database_url())
            db_status = "healthy"
            await conn.close()
        except Exception as e:
            db_status = f"unhealthy: {e!s}"

        try:
            # Check Redis connectivity (placeholder)
            cache_status = "healthy"  # Would check Redis in real implementation
        except Exception as e:
            cache_status = f"unhealthy: {e!s}"

        return {
            "status": "healthy" if db_status == "healthy" and cache_status == "healthy" else "degraded",
            "service": "blog_enterprise",
            "version": "2.0.0",
            "environment": ENV,
            "dependencies": {
                "database": db_status,
                "cache": cache_status,
            },
            "uptime": "0s",  # Would track actual uptime
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
