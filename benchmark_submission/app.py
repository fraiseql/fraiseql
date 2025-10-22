"""GraphQL Benchmark Application - FraiseQL Implementation

Production-ready FastAPI application for GraphQL benchmarking with optimal
CQRS architecture, denormalized read models, and DataLoader N+1 prevention.
"""

import os

# Import modules to register decorators
from src.models import (
    Comment,
    Direction,
    OrderBy,
    Post,
    User,
    UserFilter,
)
from src.mutations import create_post, create_user, delete_user, update_user

from fraiseql.fastapi import create_fraiseql_app

# Create the FraiseQL app with optimal configuration
app = create_fraiseql_app(
    # Database configuration from environment
    database_url=os.getenv("DATABASE_URL", "postgresql://localhost/benchmark_db"),
    # Register GraphQL types
    types=[User, Post, Comment, Direction, OrderBy, UserFilter],
    # Queries are auto-registered via @fraiseql.query decorator in queries.py
    # Mutations are explicitly registered
    mutations=[
        create_user,
        update_user,
        delete_user,
        create_post,
    ],
    # App metadata
    title="GraphQL Benchmark API",
    version="1.0.0",
    description="High-performance GraphQL API built with FraiseQL CQRS architecture",
    # Production mode from environment
    production=os.getenv("ENV") == "production",
    # Enable GraphQL playground in development
    playground_enabled=os.getenv("ENV") != "production",
    # Enable introspection in development
    introspection_enabled=os.getenv("ENV") != "production",
)


# Health check endpoint
@app.get("/health")
async def health_check():
    """Health check endpoint for load balancers and monitoring."""
    return {
        "status": "healthy",
        "service": "graphql-benchmark-api",
        "version": "1.0.0",
        "framework": "fraiseql",
    }


# Readiness check endpoint
@app.get("/ready")
async def readiness_check():
    """Readiness check endpoint for Kubernetes and orchestration."""
    # In a real application, you might check database connectivity here
    return {
        "status": "ready",
        "service": "graphql-benchmark-api",
        "endpoints": {
            "graphql": "/graphql",
            "playground": "/playground" if os.getenv("ENV") != "production" else None,
            "health": "/health",
        },
    }


# Root endpoint
@app.get("/")
async def root():
    """Root endpoint with API information."""
    return {
        "name": "GraphQL Benchmark API",
        "version": "1.0.0",
        "framework": "fraiseql",
        "architecture": "cqrs",
        "endpoints": {
            "graphql": "/graphql",
            "playground": "/playground" if os.getenv("ENV") != "production" else None,
            "health": "/health",
            "ready": "/ready",
        },
        "optimizations": [
            "CQRS with denormalized read models",
            "DataLoader for N+1 query prevention",
            "JSONB storage for optimal read performance",
            "Automatic sync triggers between write/read sides",
        ],
    }


# Configure database dependency injection for CQRS
from psycopg_pool import AsyncConnectionPool
from src.dataloaders import CommentDataLoader, PostDataLoader, UserDataLoader
from src.db import BenchmarkRepository

from fraiseql.optimization import dataloader_context

# Create connection pool with optimal settings for benchmarking
pool = AsyncConnectionPool(
    os.getenv("DATABASE_URL", "postgresql://localhost/benchmark_db"),
    min_size=int(os.getenv("DB_MIN_CONNECTIONS", "5")),
    max_size=int(os.getenv("DB_MAX_CONNECTIONS", "20")),
    max_idle_time=int(os.getenv("DB_MAX_IDLE_TIME", "300")),
)


async def get_benchmark_db():
    """Get benchmark repository for the request with DataLoader context."""
    async with pool.connection() as conn:
        # Create repository
        repo = BenchmarkRepository(conn)

        # Create DataLoaders for this request
        user_loader = UserDataLoader(repo)
        post_loader = PostDataLoader(repo)
        comment_loader = CommentDataLoader(repo)

        # Return context with repository and loaders
        yield {
            "db": repo,
            "user_loader": user_loader,
            "post_loader": post_loader,
            "comment_loader": comment_loader,
        }


# Override the default database dependency
app.dependency_overrides["db"] = get_benchmark_db


# Add DataLoader context middleware
@app.middleware("http")
async def dataloader_middleware(request, call_next):
    """Ensure DataLoader context is available for the entire request."""
    with dataloader_context():
        response = await call_next(request)
    return response


# Add request ID middleware for tracing
@app.middleware("http")
async def add_request_id(request, call_next):
    """Add request ID to all requests for tracing."""
    import uuid

    request_id = str(uuid.uuid4())
    request.state.request_id = request_id
    response = await call_next(request)
    response.headers["X-Request-ID"] = request_id
    return response


if __name__ == "__main__":
    import uvicorn

    # Run the application with optimal settings
    uvicorn.run(
        "app:app",
        host=os.getenv("HOST", "0.0.0.0"),
        port=int(os.getenv("PORT", "8000")),
        workers=int(os.getenv("WORKERS", "1")),
        reload=os.getenv("ENV") != "production",
        # Production optimizations
        loop="uvloop" if os.getenv("ENV") == "production" else "auto",
        http="httptools" if os.getenv("ENV") == "production" else "auto",
    )
