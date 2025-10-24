"""Example: Secure GraphQL API with FraiseQL Security

This example demonstrates how to set up a production-ready GraphQL API
with comprehensive security features including:

- Rate limiting for different operation types
- CSRF protection for mutations
- Security headers for defense in depth
- Input validation and sanitization

Usage:
    python secure_graphql_api.py
"""

import os
from datetime import UTC, datetime
from uuid import UUID

from fastapi import FastAPI, Request

import fraiseql
from fraiseql.security import (
    create_security_config_for_graphql,
    setup_development_security,
    setup_production_security,
    setup_security,
)

# Environment configuration
ENVIRONMENT = os.getenv("ENVIRONMENT", "development")
SECRET_KEY = os.getenv("SECRET_KEY", "dev-secret-key-change-in-production")
DOMAIN = os.getenv("DOMAIN", "api.example.com")
TRUSTED_ORIGINS = os.getenv("TRUSTED_ORIGINS", "https://app.example.com").split(",")

# Database URL (in real app, this would come from environment)
DATABASE_URL = os.getenv("DATABASE_URL", "postgresql://localhost/secure_blog")

# Redis for distributed rate limiting (optional)
REDIS_URL = os.getenv("REDIS_URL")


# GraphQL Types
@fraiseql.type
class User:
    id: UUID
    username: str
    email: str
    is_active: bool
    created_at: datetime


@fraiseql.type
class Post:
    id: UUID
    title: str
    content: str
    author_id: UUID
    published: bool
    created_at: datetime
    updated_at: datetime


@fraiseql.type
class Comment:
    id: UUID
    content: str
    post_id: UUID
    author_id: UUID
    created_at: datetime


# Input Types
@fraiseql.type
class CreatePostInput:
    title: str = fraiseql.field(description="Post title (required)")
    content: str = fraiseql.field(description="Post content (required)")
    published: bool = False


@fraiseql.type
class UpdatePostInput:
    id: UUID
    title: str | None = None
    content: str | None = None
    published: bool | None = None


@fraiseql.type
class CreateCommentInput:
    post_id: UUID
    content: str


# Result Types (for better error handling)
@fraiseql.union
class PostResult:
    success: "PostSuccess"
    error: "PostError"


@fraiseql.type
class PostSuccess:
    post: Post
    message: str


@fraiseql.type
class PostError:
    message: str
    code: str


# Root Query Type
@fraiseql.type
class Query:
    @fraiseql.field
    async def posts(
        self,
        info: fraiseql.Info,
        limit: int = 20,
        offset: int = 0,
    ) -> list[Post]:
        """Get all published posts."""
        # In a real application, this would query the database
        return []

    @fraiseql.field
    async def post(self, info: fraiseql.Info, id: UUID) -> Post | None:
        """Get a specific post by ID."""
        return None

    @fraiseql.field
    async def user(self, info: fraiseql.Info, id: UUID) -> User | None:
        """Get user by ID."""
        return None

    @fraiseql.field
    async def me(self, info: fraiseql.Info) -> User | None:
        """Get current authenticated user."""
        # Check authentication from request context
        user_id = getattr(info.context.get("request", {}).state, "user_id", None)
        if not user_id:
            return None

        # Return current user (would query database in real app)
        return None


# Mutation Type
@fraiseql.mutation
async def create_post(
    info: fraiseql.Info,
    input: CreatePostInput,
) -> PostResult:
    """Create a new post (requires authentication and CSRF token)."""
    request = info.context.get("request")

    # Check authentication
    user_id = getattr(request.state, "user_id", None)
    if not user_id:
        return PostResult(
            error=PostError(
                message="Authentication required",
                code="UNAUTHORIZED",
            ),
        )

    # Validate input (basic example)
    if len(input.title.strip()) < 3:
        return PostResult(
            error=PostError(
                message="Title must be at least 3 characters",
                code="VALIDATION_ERROR",
            ),
        )

    if len(input.content.strip()) < 10:
        return PostResult(
            error=PostError(
                message="Content must be at least 10 characters",
                code="VALIDATION_ERROR",
            ),
        )

    # In a real application, save to database
    # post = await create_post_in_db(user_id, input.title, input.content, input.published)

    # Return success result
    post = Post(
        id=UUID("123e4567-e89b-12d3-a456-426614174000"),
        title=input.title,
        content=input.content,
        author_id=user_id,
        published=input.published,
        created_at=datetime.now(tz=UTC),
        updated_at=datetime.now(tz=UTC),
    )

    return PostResult(
        success=PostSuccess(
            post=post,
            message="Post created successfully",
        ),
    )


@fraiseql.mutation
async def update_post(
    info: fraiseql.Info,
    input: UpdatePostInput,
) -> PostResult:
    """Update an existing post (requires authentication and CSRF token)."""
    request = info.context.get("request")

    # Check authentication
    user_id = getattr(request.state, "user_id", None)
    if not user_id:
        return PostResult(
            error=PostError(
                message="Authentication required",
                code="UNAUTHORIZED",
            ),
        )

    # In a real application:
    # 1. Check if post exists
    # 2. Check if user owns the post
    # 3. Update the post
    # 4. Return updated post

    return PostResult(
        error=PostError(
            message="Post not found or access denied",
            code="NOT_FOUND",
        ),
    )


@fraiseql.mutation
async def create_comment(
    info: fraiseql.Info,
    input: CreateCommentInput,
) -> Comment:
    """Create a comment on a post."""
    request = info.context.get("request")

    # Check authentication
    user_id = getattr(request.state, "user_id", None)
    if not user_id:
        raise fraiseql.GraphQLError(
            "Authentication required",
            extensions={"code": "UNAUTHORIZED"},
        )

    # Validate input
    if len(input.content.strip()) < 5:
        raise fraiseql.GraphQLError(
            "Comment must be at least 5 characters",
            extensions={"code": "VALIDATION_ERROR"},
        )

    # In a real application, save to database
    return Comment(
        id=UUID("123e4567-e89b-12d3-a456-426614174001"),
        content=input.content,
        post_id=input.post_id,
        author_id=user_id,
        created_at=datetime.now(tz=UTC),
    )


# Authentication middleware (simplified example)
async def auth_middleware(request: Request, call_next):
    """Simple authentication middleware."""
    # Check for Authorization header
    auth_header = request.headers.get("Authorization")

    if auth_header and auth_header.startswith("Bearer "):
        token = auth_header[7:]

        # In a real app, validate JWT token
        if token == "valid-token":
            request.state.user_id = UUID("123e4567-e89b-12d3-a456-426614174002")
            request.state.is_authenticated = True
        else:
            request.state.user_id = None
            request.state.is_authenticated = False
    else:
        request.state.user_id = None
        request.state.is_authenticated = False

    return await call_next(request)


def create_app() -> FastAPI:
    """Create and configure the FastAPI application."""
    # Create FastAPI app
    app = FastAPI(
        title="Secure GraphQL API",
        description="Example of a secure GraphQL API using FraiseQL",
        version="1.0.0",
    )

    # Add authentication middleware first
    app.middleware("http")(auth_middleware)

    # Set up security based on environment
    if ENVIRONMENT == "production":
        # Production security setup
        redis_client = None
        if REDIS_URL:
            try:
                import redis.asyncio as redis

                redis_client = redis.from_url(REDIS_URL)
            except ImportError:
                pass

        setup_production_security(
            app=app,
            secret_key=SECRET_KEY,
            domain=DOMAIN,
            trusted_origins=set(TRUSTED_ORIGINS),
            redis_client=redis_client,
        )

    elif ENVIRONMENT == "development":
        # Development security setup (more permissive)
        setup_development_security(
            app=app,
            secret_key=SECRET_KEY,
        )

    else:
        # Custom security setup for GraphQL
        config = create_security_config_for_graphql(
            secret_key=SECRET_KEY,
            environment=ENVIRONMENT,
            trusted_origins=TRUSTED_ORIGINS,
            enable_introspection=(ENVIRONMENT != "production"),
        )

        setup_security(
            app=app,
            secret_key=SECRET_KEY,
            custom_config=config,
        )

    # Create FraiseQL app
    fraiseql_app = fraiseql.create_fraiseql_app(
        database_url=DATABASE_URL,
        types=[User, Post, Comment, Query],
        mutations=[create_post, update_post, create_comment],
        title="Secure GraphQL API",
        description="Production-ready GraphQL API with comprehensive security",
        production=(ENVIRONMENT == "production"),
    )

    # Mount GraphQL endpoint
    app.mount("/graphql", fraiseql_app)

    # Health check endpoints
    @app.get("/health")
    async def health():
        return {"status": "healthy", "environment": ENVIRONMENT}

    @app.get("/security-info")
    async def security_info():
        """Endpoint to check security configuration (development only)."""
        if ENVIRONMENT == "production":
            return {"error": "Not available in production"}

        return {
            "environment": ENVIRONMENT,
            "security_features": {
                "rate_limiting": "enabled",
                "csrf_protection": "enabled",
                "security_headers": "enabled",
                "input_validation": "enabled",
            },
            "trusted_origins": TRUSTED_ORIGINS,
            "csrf_token_endpoint": "/csrf-token",
        }

    return app


def main():
    """Run the application."""
    import uvicorn

    app = create_app()

    # Run with uvicorn
    port = int(os.getenv("PORT", 8000))
    host = os.getenv("HOST", "0.0.0.0")  # noqa: S104

    if ENVIRONMENT != "production":
        pass

    uvicorn.run(
        app,
        host=host,
        port=port,
        reload=(ENVIRONMENT == "development"),
        log_level="info",
    )


if __name__ == "__main__":
    main()
