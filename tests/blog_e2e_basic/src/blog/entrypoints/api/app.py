"""Main application module for Blog Demo.

Following PrintOptim Backend application patterns for enterprise-grade
FastAPI + FraiseQL integration with proper middleware, security, and configuration.
"""

import logging
import uuid
from typing import Any, Dict, Optional
from contextlib import asynccontextmanager

import psycopg
from fastapi import FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.middleware.trustedhost import TrustedHostMiddleware
from fastapi.responses import JSONResponse

import fraiseql
from fraiseql import FraiseQL
from fraiseql.cqrs import CQRSRepository
from fraiseql.fastapi import mount_fraiseql

from ...config import config
from ...core.exceptions import BlogException
from .middleware.security import SecurityMiddleware
from .middleware.error_handler import ErrorHandlerMiddleware
from . import gql_types


# Configure logging
logging.basicConfig(
    level=getattr(logging, config.log_level.upper()),
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s"
)
logger = logging.getLogger(__name__)


# Global database connection pool
db_pool: Optional[Any] = None


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Application lifespan manager for database connections."""
    global db_pool

    # Startup
    logger.info("Starting Blog Demo Application")

    # Initialize database connection pool
    try:
        db_pool = await psycopg.AsyncConnectionPool.create(
            conninfo=config.database_url,
            min_size=config.database_pool_size,
            max_size=config.database_pool_size + config.database_max_overflow,
            timeout=config.database_pool_timeout,
            server_settings={
                "application_name": "blog_demo",
                "timezone": "UTC"
            }
        )
        logger.info("Database connection pool initialized")
    except Exception as e:
        logger.error(f"Failed to initialize database pool: {e}")
        raise

    yield

    # Shutdown
    logger.info("Shutting down Blog Demo Application")
    if db_pool:
        await db_pool.close()
        logger.info("Database connection pool closed")


def create_app() -> FastAPI:
    """Create and configure the FastAPI application."""

    # Create FastAPI app
    app = FastAPI(
        title="Blog Demo API",
        description="Enterprise Blog Demo built with FraiseQL",
        version="1.0.0",
        docs_url="/docs" if config.debug else None,
        redoc_url="/redoc" if config.debug else None,
        lifespan=lifespan
    )

    # Add security middleware
    if not config.debug:
        app.add_middleware(
            TrustedHostMiddleware,
            allowed_hosts=["localhost", "127.0.0.1", "*.example.com"]
        )

    # Add CORS middleware
    app.add_middleware(
        CORSMiddleware,
        allow_origins=config.cors_origins or ["*"],
        allow_credentials=True,
        allow_methods=["GET", "POST", "PUT", "DELETE", "OPTIONS"],
        allow_headers=["*"],
    )

    # Add custom middleware
    app.add_middleware(SecurityMiddleware)
    app.add_middleware(ErrorHandlerMiddleware)

    # Create FraiseQL app
    fraiseql_app = create_fraiseql_app()

    # Mount FraiseQL GraphQL endpoint
    mount_fraiseql(
        app,
        fraiseql_app,
        path=config.graphql_path,
        playground=config.graphql_playground,
        introspection=config.graphql_introspection
    )

    # Health check endpoint
    @app.get("/health")
    async def health_check():
        """Health check endpoint."""
        try:
            # Test database connection
            if db_pool:
                async with db_pool.connection() as conn:
                    await conn.execute("SELECT 1")

            return {
                "status": "healthy",
                "service": "blog_demo",
                "version": "1.0.0",
                "environment": config.environment
            }
        except Exception as e:
            logger.error(f"Health check failed: {e}")
            raise HTTPException(status_code=503, detail="Service unhealthy")

    # Blog-specific endpoints
    @app.get("/")
    async def root():
        """Root endpoint with API information."""
        return {
            "message": "Blog Demo API",
            "version": "1.0.0",
            "graphql_endpoint": config.graphql_path,
            "playground": config.graphql_playground,
            "documentation": "/docs" if config.debug else "Contact administrator"
        }

    return app


def create_fraiseql_app() -> FraiseQL:
    """Create and configure the FraiseQL application."""

    # Create FraiseQL instance
    fraiseql_app = FraiseQL(
        config=config.to_fraiseql_config()
    )

    # Context getter following PrintOptim Backend patterns
    async def get_context(request: Request) -> Dict[str, Any]:
        """Get request context for GraphQL operations."""
        global db_pool

        if not db_pool:
            raise BlogException("Database pool not initialized")

        # Get database connection from pool
        conn = await db_pool.getconn()
        repo = CQRSRepository(conn)

        # Extract user context (simplified for demo)
        user_id = extract_user_id(request)
        organization_id = extract_organization_id(request)

        context = {
            # Database access
            "db": repo,
            "db_connection": conn,

            # User context
            "current_user_id": user_id,
            "user_id": user_id,
            "current_organization_id": organization_id,
            "organization_id": organization_id,
            "tenant_id": organization_id,  # For multi-tenancy

            # Request context
            "request": request,
            "user_agent": request.headers.get("user-agent", ""),
            "ip_address": request.client.host if request.client else "unknown",

            # Application context
            "environment": config.environment,
            "debug": config.debug,
        }

        return context

    # Set context getter
    fraiseql_app.context_getter = get_context

    # Register all GraphQL types and mutations
    register_graphql_types(fraiseql_app)

    return fraiseql_app


def extract_user_id(request: Request) -> uuid.UUID:
    """Extract user ID from request.

    In a real application, this would extract from JWT token, session, etc.
    For the demo, we use a test user ID.
    """
    # Check for test user ID in headers (for testing)
    test_user_id = request.headers.get("X-Test-User-ID")
    if test_user_id:
        try:
            return uuid.UUID(test_user_id)
        except ValueError:
            pass

    # Default test user
    return uuid.UUID("11111111-1111-1111-1111-111111111111")


def extract_organization_id(request: Request) -> uuid.UUID:
    """Extract organization/tenant ID from request."""
    # Check for test organization ID in headers
    test_org_id = request.headers.get("X-Test-Organization-ID")
    if test_org_id:
        try:
            return uuid.UUID(test_org_id)
        except ValueError:
            pass

    # Default test organization
    return uuid.UUID("22222222-2222-2222-2222-222222222222")


def register_graphql_types(fraiseql_app: FraiseQL) -> None:
    """Register all GraphQL types and mutations with FraiseQL."""

    # Import and register all types
    from .gql_types import content, users, taxonomy, comments

    # Content types
    fraiseql_app.add_type(content.CreatePost)
    fraiseql_app.add_type(content.UpdatePost)
    fraiseql_app.add_type(content.DeletePost)
    fraiseql_app.add_type(content.PublishPost)

    # User types
    fraiseql_app.add_type(users.CreateAuthor)
    fraiseql_app.add_type(users.UpdateAuthor)
    fraiseql_app.add_type(users.DeleteAuthor)

    # Taxonomy types
    fraiseql_app.add_type(taxonomy.CreateTag)
    fraiseql_app.add_type(taxonomy.UpdateTag)
    fraiseql_app.add_type(taxonomy.DeleteTag)

    # Comment types
    fraiseql_app.add_type(comments.CreateComment)
    fraiseql_app.add_type(comments.UpdateComment)
    fraiseql_app.add_type(comments.DeleteComment)
    fraiseql_app.add_type(comments.ApproveComment)

    # Register input/output types
    register_io_types(fraiseql_app)

    logger.info("All GraphQL types registered successfully")


def register_io_types(fraiseql_app: FraiseQL) -> None:
    """Register input and output types."""

    # Import all IO types
    from .gql_types.content import (
        CreatePostInput, CreatePostSuccess, CreatePostError,
        UpdatePostInput, UpdatePostSuccess, UpdatePostError,
        PublishPostInput, PublishPostSuccess, PublishPostError,
        Post
    )
    from .gql_types.users import (
        CreateAuthorInput, CreateAuthorSuccess, CreateAuthorError,
        Author
    )
    from .gql_types.taxonomy import (
        CreateTagInput, CreateTagSuccess, CreateTagError,
        Tag
    )
    from .gql_types.comments import (
        CreateCommentInput, CreateCommentSuccess, CreateCommentError,
        Comment
    )

    # Register all types
    types_to_register = [
        # Content
        CreatePostInput, CreatePostSuccess, CreatePostError,
        UpdatePostInput, UpdatePostSuccess, UpdatePostError,
        PublishPostInput, PublishPostSuccess, PublishPostError,
        Post,

        # Users
        CreateAuthorInput, CreateAuthorSuccess, CreateAuthorError,
        Author,

        # Taxonomy
        CreateTagInput, CreateTagSuccess, CreateTagError,
        Tag,

        # Comments
        CreateCommentInput, CreateCommentSuccess, CreateCommentError,
        Comment,
    ]

    for type_class in types_to_register:
        fraiseql_app.add_type(type_class)


# Create the application instance
app = create_app()


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(
        "app:app",
        host=config.api_host,
        port=config.api_port,
        reload=config.debug,
        log_level=config.log_level.lower()
    )
