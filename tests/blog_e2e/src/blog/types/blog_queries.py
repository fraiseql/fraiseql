"""Blog Query Resolvers - Clean FraiseQL Patterns

Demonstrates clean query patterns with proper database integration
and performance optimization.
"""

from typing import List, Optional

import fraiseql
from .blog_types import Post, Author, PostFilterInput, PostSortInput, PostConnection, BlogStats


# ============================================================================
# POST QUERIES - Content retrieval
# ============================================================================

@fraiseql.query
async def posts(
    info,
    limit: int = 10,
    offset: int = 0,
    filters: Optional[PostFilterInput] = None,
    sort: Optional[PostSortInput] = None
) -> List[Post]:
    """Get blog posts with advanced filtering and sorting.

    🎯 Features:
    - Advanced filtering by status, author, tags, dates
    - Full-text search capability
    - Performance optimized with materialized views
    - Automatic pagination support
    """

    repo = info.context["db"]

    # Build filter conditions
    where_conditions = {}
    if filters:
        if filters.status:
            where_conditions["status"] = filters.status
        if filters.author_identifier:
            where_conditions["author_identifier"] = filters.author_identifier
        if filters.tag_identifier:
            where_conditions["tag_identifier"] = filters.tag_identifier
        if filters.search_query:
            where_conditions["search_query"] = filters.search_query

    # Build sort order
    order_by = "created_at DESC"  # Default
    if sort:
        direction = "ASC" if sort.direction.upper() == "ASC" else "DESC"
        order_by = f"{sort.field} {direction}"

    # Use materialized view for performance (tv_post)
    results = await repo.find(
        "tv_post",
        limit=limit,
        offset=offset,
        order_by=order_by,
        **where_conditions
    )

    # FraiseQL automatically instantiates from the 'data' JSONB column
    return results


@fraiseql.query
async def post(info, identifier: str) -> Optional[Post]:
    """Get a single post by identifier.

    🎯 Optimizations:
    - Uses real-time view (v_post) for immediate consistency
    - Includes author and tag information
    - Tracks view count automatically
    """

    repo = info.context["db"]

    # Use real-time view for single post lookup
    result = await repo.find_one("v_post", identifier=identifier)

    if result:
        # Increment view count asynchronously (fire and forget)
        await repo.call_function("app.increment_post_view", post_id=result["id"])
        # FraiseQL automatically instantiates from the 'data' JSONB column
        return result

    return None


@fraiseql.query
async def published_posts(
    info,
    limit: int = 10,
    offset: int = 0,
    tag: Optional[str] = None
) -> List[Post]:
    """Get published posts for public consumption.

    🎯 Public API:
    - Only returns published posts
    - Optimized for public website performance
    - Optional tag filtering
    """

    repo = info.context["db"]

    where_conditions = {"status": "published"}
    if tag:
        where_conditions["tag_identifier"] = tag

    results = await repo.find(
        "tv_post",
        limit=limit,
        offset=offset,
        order_by="published_at DESC",
        **where_conditions
    )

    # FraiseQL automatically instantiates from the 'data' JSONB column
    return results


# ============================================================================
# AUTHOR QUERIES - Author management
# ============================================================================

@fraiseql.query
async def authors(
    info,
    limit: int = 20,
    offset: int = 0,
    active_only: bool = False
) -> List[Author]:
    """Get blog authors with statistics.

    🎯 Features:
    - Includes post counts and activity stats
    - Optional filtering for active authors only
    - Sorted by activity level
    """

    repo = info.context["db"]

    where_conditions = {}
    if active_only:
        where_conditions["has_published_posts"] = True

    results = await repo.find(
        "tv_author",  # Materialized view with stats
        limit=limit,
        offset=offset,
        order_by="published_post_count DESC, created_at DESC",
        **where_conditions
    )

    # FraiseQL automatically instantiates from the 'data' JSONB column
    return results


@fraiseql.query
async def author(info, identifier: str) -> Optional[Author]:
    """Get a single author by identifier."""

    repo = info.context["db"]

    result = await repo.find_one("v_author", identifier=identifier)

    # FraiseQL automatically instantiates from the 'data' JSONB column
    return result


# ============================================================================
# ANALYTICS QUERIES - Dashboard and insights
# ============================================================================

@fraiseql.query
async def blog_stats(info) -> BlogStats:
    """Get comprehensive blog statistics.

    🎯 Dashboard Data:
    - Overall content statistics
    - Engagement metrics
    - Recent activity trends
    """

    repo = info.context["db"]

    # Get stats from dedicated view
    result = await repo.find_one("v_blog_stats")

    return BlogStats(
        total_posts=result["total_posts"],
        published_posts=result["published_posts"],
        draft_posts=result["draft_posts"],
        total_authors=result["total_authors"],
        total_comments=result["total_comments"],
        total_tags=result["total_tags"],
        total_views=result["total_views"],
        total_likes=result["total_likes"],
        posts_this_month=result["posts_this_month"],
        comments_this_month=result["comments_this_month"],
        last_updated=result["last_updated"]
    )


# ============================================================================
# SEARCH QUERIES - Full-text search
# ============================================================================

@fraiseql.query
async def search_posts(
    info,
    query: str,
    limit: int = 10,
    offset: int = 0
) -> List[Post]:
    """Full-text search across blog posts.

    🎯 Search Features:
    - Searches title, content, and excerpt
    - Relevance-based sorting
    - Highlighted results (in full implementation)
    """

    repo = info.context["db"]

    # Use PostgreSQL full-text search
    results = await repo.call_function(
        "app.search_posts",
        search_query=query,
        limit=limit,
        offset=offset
    )

    # FraiseQL automatically instantiates from the 'data' JSONB column
    return results


# ============================================================================
# FEED QUERIES - RSS/Atom feed support
# ============================================================================

@fraiseql.query
async def recent_posts_feed(
    info,
    limit: int = 20
) -> List[Post]:
    """Get recent published posts for RSS/Atom feeds.

    🎯 Feed Optimization:
    - Only published posts
    - Optimized for feed readers
    - Includes full content and metadata
    """

    repo = info.context["db"]

    results = await repo.find(
        "tv_post",
        limit=limit,
        status="published",
        order_by="published_at DESC"
    )

    # FraiseQL automatically instantiates from the 'data' JSONB column
    return results


# ============================================================================
# ADMIN QUERIES - Content management
# ============================================================================

@fraiseql.query
async def draft_posts(info, author_id: Optional[str] = None) -> List[Post]:
    """Get draft posts for content management.

    🎯 Admin Features:
    - Draft and pending posts
    - Optional author filtering
    - Sorted by last modified
    """

    repo = info.context["db"]

    where_conditions = {"status": "draft"}
    if author_id:
        where_conditions["author_id"] = author_id

    results = await repo.find(
        "v_post",  # Real-time view for admin
        order_by="updated_at DESC",
        **where_conditions
    )

    # FraiseQL automatically instantiates from the 'data' JSONB column
    return results
