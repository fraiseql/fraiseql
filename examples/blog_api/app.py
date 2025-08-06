"""Example blog API application using FraiseQL with CQRS."""

import os

# Import queries module to ensure @query decorators are registered
import queries
from models import Comment, Post, User
from mutations import (
    create_comment,
    create_post,
    create_user,
    delete_post,
    update_post,
)

from fraiseql.fastapi import create_fraiseql_app

# Create the FraiseQL app
app = create_fraiseql_app(
    # Database configuration from environment
    database_url=os.getenv("DATABASE_URL", "postgresql://localhost/blog_db"),
    # Register GraphQL types
    types=[User, Post, Comment],
    # Queries are auto-registered via @fraiseql.query decorator in queries.py
    # No need to list them explicitly!
    # Register mutations
    mutations=[
        create_user,
        create_post,
        update_post,
        create_comment,
        delete_post,
    ],
    # Auth0 configuration disabled for example
    auth=None,
    # App metadata
    title="Blog API",
    version="1.0.0",
    description="A simple blog API built with FraiseQL",
    # Production mode from environment
    production=os.getenv("ENV") == "production",
)


# Add custom endpoints if needed
@app.get("/")
async def root():
    """Root endpoint."""
    return {
        "name": "Blog API",
        "version": "1.0.0",
        "graphql": "/graphql",
        "playground": "/playground" if os.getenv("ENV") != "production" else None,
    }


# Configure database dependency injection for CQRS
from db import BlogRepository
from psycopg_pool import AsyncConnectionPool

# Create connection pool
pool = AsyncConnectionPool(
    os.getenv("DATABASE_URL", "postgresql://localhost/blog_db"),
    min_size=5,
    max_size=20,
)


# Override the default database dependency
async def get_blog_db():
    """Get blog repository for the request."""
    async with pool.connection() as conn:
        yield BlogRepository(conn)


# Override the db dependency
app.dependency_overrides["db"] = get_blog_db


# Example of adding custom middleware
@app.middleware("http")
async def add_request_id(request, call_next):
    """Add request ID to all requests."""
    import uuid

    request_id = str(uuid.uuid4())
    request.state.request_id = request_id
    response = await call_next(request)
    response.headers["X-Request-ID"] = request_id
    return response


if __name__ == "__main__":
    import uvicorn

    # Run the application
    uvicorn.run(
        "app:app",
        host="0.0.0.0",  # noqa: S104
        port=8000,
        reload=os.getenv("ENV") != "production",
    )
