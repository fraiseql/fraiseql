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


def create_app() -> FastAPI:
    """Create the simple blog FastAPI application."""
    app = FastAPI(
        title="FraiseQL Simple Blog",
        description="Simple blog built with FraiseQL",
        version="1.0.0",
        lifespan=lifespan,
    )

    # CORS setup for development
    app.add_middleware(
        CORSMiddleware,
        allow_origins=["http://localhost:3000", "http://127.0.0.1:3000"],
        allow_credentials=True,
        allow_methods=["GET", "POST"],
        allow_headers=["*"],
    )

    # Context getter for GraphQL
    async def get_context(request: Request) -> Dict[str, Any]:
        """Provide context for GraphQL operations."""
        database_url = get_database_url()

        try:
            conn = await psycopg.AsyncConnection.connect(database_url)

            return {
                "db": CQRSRepository(conn),
                "user_id": uuid.UUID("11111111-1111-1111-1111-111111111111"),  # Demo user
                "tenant_id": uuid.UUID("22222222-2222-2222-2222-222222222222"),  # Demo tenant
                "request": request,
            }
        except Exception as e:
            logger.error(f"Failed to create database connection: {e}")
            raise

    # Import blog schema
    from models import BLOG_MUTATIONS, BLOG_QUERIES, BLOG_TYPES

    # Create FraiseQL app
    fraiseql_app = create_fraiseql_app(
        database_url=get_database_url(),
        types=BLOG_TYPES,
        mutations=BLOG_MUTATIONS,
        queries=BLOG_QUERIES,
        context_getter=get_context,
        title="FraiseQL Blog API",
        description="Simple blog API demonstrating FraiseQL patterns",
        production=False,  # Enable playground in development
    )

    # Mount GraphQL endpoint
    app.mount("/graphql", fraiseql_app)

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

    @app.get("/health")
    async def health():
        return {"status": "healthy", "service": "blog_simple"}

    return app


# Create the app instance
app = create_app()

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000, reload=True)
