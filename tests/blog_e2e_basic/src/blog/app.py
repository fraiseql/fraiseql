"""Blog Demo Application - FraiseQL Enterprise Showcase

A streamlined blog application demonstrating FraiseQL's enterprise readiness:
- Clean default mutation patterns (no "Enhanced"/"Optimized" prefixes)
- Database-first architecture with PostgreSQL functions
- Comprehensive error handling with native error arrays
- Auto-decorated success/failure types
- Production-ready patterns with smooth DX
"""

import uuid
import logging
from typing import Dict, Any, Optional
from contextlib import asynccontextmanager

import psycopg
from fastapi import FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware

import fraiseql
from fraiseql.errors import FraiseQLError
from fraiseql.cqrs import CQRSRepository
from fraiseql.fastapi import create_fraiseql_app


# Configure logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

# Global database pool
db_pool: Optional[Any] = None


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Application lifespan with database connection management."""
    global db_pool

    # Startup
    logger.info("🚀 Starting FraiseQL Blog Demo")

    # Initialize database pool
    db_pool = await psycopg.AsyncConnectionPool.create(
        "postgresql://postgres:postgres@localhost:5432/blog_demo",
        min_size=5,
        max_size=20
    )
    logger.info("✅ Database pool initialized")

    yield

    # Shutdown
    if db_pool:
        await db_pool.close()
        logger.info("🔒 Database pool closed")


def create_app() -> FastAPI:
    """Create the streamlined blog demo application."""

    app = FastAPI(
        title="FraiseQL Blog Demo",
        description="Enterprise-ready blog built with FraiseQL clean patterns",
        version="1.0.0",
        lifespan=lifespan
    )

    # Simple CORS setup
    app.add_middleware(
        CORSMiddleware,
        allow_origins=["*"],
        allow_credentials=True,
        allow_methods=["*"],
        allow_headers=["*"]
    )

    # Create FraiseQL app with clean patterns
    fraiseql_app = create_fraiseql_app()

    # Context getter - clean and simple
    async def get_context(request: Request) -> Dict[str, Any]:
        """Provide clean context for GraphQL operations."""
        conn = await db_pool.getconn()

        return {
            "db": CQRSRepository(conn),
            "user_id": uuid.UUID("11111111-1111-1111-1111-111111111111"),  # Demo user
            "organization_id": uuid.UUID("22222222-2222-2222-2222-222222222222"),  # Demo org
            "request": request
        }

    fraiseql_app.context_getter = get_context

    # Register GraphQL types
    register_blog_types(fraiseql_app)

    # Mount GraphQL endpoint
    app.mount("/graphql", fraiseql_app)

    @app.get("/")
    async def home():
        return {
            "message": "🎉 FraiseQL Blog Demo - Enterprise Ready!",
            "features": [
                "Clean default mutation patterns",
                "Database-first architecture",
                "Auto-decorated types",
                "Native error arrays",
                "Comprehensive error handling"
            ],
            "graphql": "/graphql"
        }

    @app.get("/health")
    async def health():
        return {"status": "healthy", "service": "blog_demo"}

    return app


def register_blog_types(fraiseql_app: fraiseql.FraiseQL):
    """Register all blog GraphQL types showcasing FraiseQL patterns."""

    # Import and register types
    from .types import blog_mutations, blog_types, blog_queries

    # Register all GraphQL types and inputs

    # Core entity types
    fraiseql_app.add_type(blog_types.Post)
    fraiseql_app.add_type(blog_types.Author)
    fraiseql_app.add_type(blog_types.Tag)
    fraiseql_app.add_type(blog_types.Comment)
    fraiseql_app.add_type(blog_types.BlogStats)
    fraiseql_app.add_type(blog_types.PopularContent)

    # Input types
    fraiseql_app.add_type(blog_types.PostFilterInput)
    fraiseql_app.add_type(blog_types.PostSortInput)
    fraiseql_app.add_type(blog_types.PaginationInfo)
    fraiseql_app.add_type(blog_types.PostConnection)
    fraiseql_app.add_type(blog_types.SearchHighlight)
    fraiseql_app.add_type(blog_types.SearchResult)
    fraiseql_app.add_type(blog_types.SearchResponse)

    # Mutation input types
    fraiseql_app.add_type(blog_mutations.CreatePostInput)
    fraiseql_app.add_type(blog_mutations.UpdatePostInput)
    fraiseql_app.add_type(blog_mutations.CreateAuthorInput)

    # Success/Error types
    fraiseql_app.add_type(blog_mutations.CreatePostSuccess)
    fraiseql_app.add_type(blog_mutations.CreatePostError)
    fraiseql_app.add_type(blog_mutations.UpdatePostSuccess)
    fraiseql_app.add_type(blog_mutations.UpdatePostError)
    fraiseql_app.add_type(blog_mutations.PublishPostSuccess)
    fraiseql_app.add_type(blog_mutations.PublishPostError)
    fraiseql_app.add_type(blog_mutations.CreateAuthorSuccess)
    fraiseql_app.add_type(blog_mutations.CreateAuthorError)

    # Mutations (showcasing clean patterns)
    fraiseql_app.add_type(blog_mutations.CreatePost)
    fraiseql_app.add_type(blog_mutations.UpdatePost)
    fraiseql_app.add_type(blog_mutations.PublishPost)
    fraiseql_app.add_type(blog_mutations.CreateAuthor)

    # Query resolvers
    fraiseql_app.add_type(blog_queries.posts)
    fraiseql_app.add_type(blog_queries.post)
    fraiseql_app.add_type(blog_queries.published_posts)
    fraiseql_app.add_type(blog_queries.authors)
    fraiseql_app.add_type(blog_queries.author)
    fraiseql_app.add_type(blog_queries.blog_stats)
    fraiseql_app.add_type(blog_queries.search_posts)
    fraiseql_app.add_type(blog_queries.recent_posts_feed)
    fraiseql_app.add_type(blog_queries.draft_posts)

    logger.info("✅ All GraphQL types registered")


# Create the app instance
app = create_app()


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000, reload=True)
