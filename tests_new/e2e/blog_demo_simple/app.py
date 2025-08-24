"""Simple Blog Demo Application - FraiseQL Integration

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
from typing import Any, Dict

import psycopg
from fastapi import FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware

from fraiseql.cqrs import CQRSRepository
from fraiseql.fastapi import create_fraiseql_app

# Configure logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

# Database configuration from environment
DB_NAME = os.getenv("DB_NAME", "fraiseql_blog_simple_test")
DB_USER = os.getenv("DB_USER", "lionel")
DB_PASSWORD = os.getenv("DB_PASSWORD", "")
DB_HOST = os.getenv("DB_HOST", "localhost")
DB_PORT = int(os.getenv("DB_PORT", "5432"))


def get_database_url() -> str:
    """Get database URL from environment variables."""
    if DB_PASSWORD:
        return f"postgresql://{DB_USER}:{DB_PASSWORD}@{DB_HOST}:{DB_PORT}/{DB_NAME}"
    return f"postgresql://{DB_USER}@{DB_HOST}:{DB_PORT}/{DB_NAME}"


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Application lifespan with database connection management."""
    logger.info("🚀 Starting FraiseQL Simple Blog Demo")

    # Initialize database pool
    database_url = get_database_url()
    logger.info(f"Connecting to database: {database_url}")

    yield

    logger.info("🔒 Simple Blog Demo shutdown")


def create_app() -> FastAPI:
    """Create the simple blog demo FastAPI application."""
    app = FastAPI(
        title="FraiseQL Simple Blog Demo",
        description="Simple blog built with FraiseQL for testing",
        version="1.0.0",
        lifespan=lifespan,
    )

    # Simple CORS setup for testing
    app.add_middleware(
        CORSMiddleware,
        allow_origins=["*"],
        allow_credentials=True,
        allow_methods=["*"],
        allow_headers=["*"],
    )

    # Context getter for GraphQL
    async def get_context(request: Request) -> Dict[str, Any]:
        """Provide context for GraphQL operations."""
        # For testing, create connection from database URL
        database_url = get_database_url()

        try:
            conn = await psycopg.AsyncConnection.connect(database_url)

            return {
                "db": CQRSRepository(conn),
                "user_id": uuid.UUID("11111111-1111-1111-1111-111111111111"),  # Test user
                "tenant_id": uuid.UUID("22222222-2222-2222-2222-222222222222"),  # Test tenant
                "request": request,
            }
        except Exception as e:
            logger.error(f"Failed to create database connection: {e}")
            # Return minimal context for testing
            return {
                "user_id": uuid.UUID("11111111-1111-1111-1111-111111111111"),
                "tenant_id": uuid.UUID("22222222-2222-2222-2222-222222222222"),
                "request": request,
            }

    # Import blog schema
    from blog_schema import BLOG_MUTATIONS, BLOG_QUERIES, BLOG_TYPES

    # Create FraiseQL app with full blog schema
    fraiseql_app = create_fraiseql_app(
        database_url=get_database_url(),
        types=BLOG_TYPES,
        mutations=BLOG_MUTATIONS,
        queries=BLOG_QUERIES,
        context_getter=get_context,
        title="FraiseQL Blog Demo API",
        description="Blog demo API with real database integration",
        production=False,  # Development mode for testing
    )

    # Mount GraphQL endpoint
    app.mount("/graphql", fraiseql_app)

    @app.get("/")
    async def home():
        return {
            "message": "🎉 FraiseQL Simple Blog Demo",
            "features": [
                "Real PostgreSQL database integration",
                "GraphQL API with FraiseQL",
                "Test-friendly architecture",
                "Database fixtures support",
            ],
            "endpoints": {
                "graphql": "/graphql",
                "playground": "/graphql" if os.getenv("ENV") != "production" else None,
            },
        }

    @app.get("/health")
    async def health():
        return {"status": "healthy", "service": "simple_blog_demo"}

    return app


# Create the app instance for testing
app = create_app()

if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000, reload=True)
