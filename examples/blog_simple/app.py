"""FraiseQL Blog Simple - Complete Example Application

A simple blog application demonstrating FraiseQL's core capabilities:
- Real PostgreSQL database integration
- GraphQL API with mutations and queries
- Test-friendly architecture with database fixtures
- Clean error handling patterns
"""

import logging
import os
import uuid
from contextlib import asynccontextmanager
from typing import Any

import psycopg
from fastapi import FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware

from fraiseql.cqrs import CQRSRepository
from fraiseql.fastapi import create_fraiseql_app

# Configure logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

# Database configuration from environment
DB_NAME = os.getenv("DB_NAME", "fraiseql_blog_simple")
DB_USER = os.getenv("DB_USER", "fraiseql")
DB_PASSWORD = os.getenv("DB_PASSWORD", "fraiseql")
DB_HOST = os.getenv("DB_HOST", "localhost")
DB_PORT = int(os.getenv("DB_PORT", "5432"))


def get_database_url() -> str:
    """Get database URL from environment variables."""
    return f"postgresql://{DB_USER}:{DB_PASSWORD}@{DB_HOST}:{DB_PORT}/{DB_NAME}"


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Application lifespan with database connection management."""
    logger.info("🚀 Starting FraiseQL Simple Blog")
    logger.info(f"Connecting to database: {get_database_url()}")
    yield
    logger.info("🔒 Simple Blog shutdown")


def _create_base_app() -> FastAPI:
    """Create the base simple blog FastAPI application."""

    # Context getter for GraphQL - use FraiseQL's database dependency
    async def get_context(request: Request) -> dict[str, Any]:
        """Provide context for GraphQL operations."""
        # Import here to avoid circular imports
        from fraiseql.fastapi.dependencies import get_db

        try:
            # Use FraiseQL's database dependency to get a connection from the pool
            db = await get_db()

            return {
                "db": db,
                "user_id": uuid.UUID("11111111-1111-1111-1111-111111111111"),  # Demo user
                "tenant_id": uuid.UUID("22222222-2222-2222-2222-222222222222"),  # Demo tenant
                "request": request,
            }
        except Exception as e:
            logger.error(f"Failed to create database connection: {e}")
            raise

    # Import blog schema
    try:
        # Try local import first (when running from blog_simple directory)
        from models import BLOG_MUTATIONS, BLOG_QUERIES, BLOG_TYPES
    except ImportError:
        # Fallback for when running from repository root (CI environment)
        from examples.blog_simple.models import BLOG_MUTATIONS, BLOG_QUERIES, BLOG_TYPES

    # Create FraiseQL app directly - this becomes our main app
    app = create_fraiseql_app(
        database_url=get_database_url(),
        types=BLOG_TYPES,
        mutations=BLOG_MUTATIONS,
        queries=BLOG_QUERIES,
        # context_getter=get_context,  # Disable custom context for now
        title="FraiseQL Simple Blog",
        description="Simple blog built with FraiseQL",
        production=False,  # Enable playground in development
    )

    # Add CORS for development (FraiseQL may already handle this)
    app.add_middleware(
        CORSMiddleware,
        allow_origins=["http://localhost:3000", "http://127.0.0.1:3000"],
        allow_credentials=True,
        allow_methods=["GET", "POST"],
        allow_headers=["*"],
    )

    # Override the default home endpoint
    @app.get("/")
    async def home():
        return {
            "message": "🎉 FraiseQL Simple Blog",
            "description": "A complete blog example built with FraiseQL",
            "features": [
                "Real PostgreSQL database integration",
                "GraphQL API with FraiseQL",
                "CRUD operations with error handling",
                "Test-friendly architecture",
            ],
            "endpoints": {
                "graphql": "/graphql",
                "playground": "/graphql",
            },
        }

    return app


def create_app():
    """Main application factory with custom health endpoint override."""
    # Create the base FraiseQL app first
    app = _create_base_app()

    # Override FraiseQL's health endpoint by adding our own after app creation
    async def custom_health():
        import logging

        logger = logging.getLogger(__name__)
        logger.info("CUSTOM HEALTH ENDPOINT CALLED - blog_simple")
        return {"status": "healthy", "service": "blog_simple"}

    # Find and replace the existing health route
    import logging

    logger = logging.getLogger(__name__)
    health_routes_found = 0
    for i, route in enumerate(app.routes):
        if hasattr(route, "path") and route.path == "/health":
            health_routes_found += 1
            logger.info(f"Found health route {health_routes_found} at index {i}: {route}")
            # Create new route with our endpoint
            from fastapi.routing import APIRoute

            new_route = APIRoute("/health", custom_health, methods=["GET"])
            # Replace the route at the same position
            routes_list = list(app.routes)
            routes_list[i] = new_route
            app.router.routes = routes_list
            logger.info(f"Replaced health route {health_routes_found} with custom endpoint")

    if health_routes_found == 0:
        logger.info("No health route found, adding custom health endpoint")
        from fastapi.routing import APIRoute

        new_route = APIRoute("/health", custom_health, methods=["GET"])
        app.router.routes.append(new_route)

    return app


# Create the app instance
app = create_app()

if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000, reload=True)
