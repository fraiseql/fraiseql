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
from typing import Dict, Any
from contextlib import asynccontextmanager

from fastapi import FastAPI, Request

from fraiseql.cqrs import CQRSRepository
from fraiseql.fastapi import create_fraiseql_app


# Configure logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)


def create_app() -> FastAPI:
    """Create the streamlined blog demo application."""

    # Import types here to avoid circular imports
    from .types import blog_mutations, blog_types, blog_queries

    # Collect all types for registration
    all_types = [
        # Core entity types
        blog_types.Post,
        blog_types.Author,
        blog_types.Tag,
        blog_types.Comment,
        blog_types.BlogStats,
        blog_types.PopularContent,
        # Input types
        blog_types.PostFilterInput,
        blog_types.PostSortInput,
        blog_types.PaginationInfo,
        blog_types.PostConnection,
        blog_types.SearchHighlight,
        blog_types.SearchResult,
        blog_types.SearchResponse,
        # Mutation input types
        blog_mutations.CreatePostInput,
        blog_mutations.UpdatePostInput,
        blog_mutations.CreateAuthorInput,
        # Success/Error types
        blog_mutations.CreatePostSuccess,
        blog_mutations.CreatePostError,
        blog_mutations.UpdatePostSuccess,
        blog_mutations.UpdatePostError,
        blog_mutations.PublishPostSuccess,
        blog_mutations.PublishPostError,
        blog_mutations.CreateAuthorSuccess,
        blog_mutations.CreateAuthorError,
        # Mutations (showcasing clean patterns)
        blog_mutations.CreatePost,
        blog_mutations.UpdatePost,
        blog_mutations.PublishPost,
        blog_mutations.CreateAuthor,
    ]

    # Collect all query and mutation functions
    all_mutations = []

    all_queries = [
        blog_queries.posts,
        blog_queries.post,
        blog_queries.published_posts,
        blog_queries.authors,
        blog_queries.author,
        blog_queries.blog_stats,
        blog_queries.search_posts,
        blog_queries.recent_posts_feed,
        blog_queries.draft_posts,
    ]

    # Context getter - clean and simple
    async def get_context(request: Request) -> Dict[str, Any]:
        """Provide clean context for GraphQL operations."""
        # In the real blog demo, this would get connection from the pool
        # For testing, we'll return a basic context
        return {
            "user_id": uuid.UUID("11111111-1111-1111-1111-111111111111"),  # Demo user
            "organization_id": uuid.UUID("22222222-2222-2222-2222-222222222222"),  # Demo org
            "request": request
        }

    # Create FraiseQL app with all types registered
    app = create_fraiseql_app(
        database_url="postgresql://postgres:postgres@localhost:5432/blog_demo",
        types=all_types,
        mutations=all_mutations,
        queries=all_queries,
        context_getter=get_context,
        title="FraiseQL Blog Demo",
        description="Enterprise-ready blog built with FraiseQL clean patterns",
        version="1.0.0",
    )

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

    logger.info("✅ FraiseQL Blog Demo created with modern API")
    return app


# Create the app instance
app = create_app()


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000, reload=True)
