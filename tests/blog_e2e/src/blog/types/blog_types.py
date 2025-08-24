"""Blog GraphQL Types - Clean and Enterprise-Ready

Showcases FraiseQL's clean type definitions with proper enterprise patterns.
"""

import uuid
from typing import List, Optional
from datetime import datetime

import fraiseql


# ============================================================================
# CORE ENTITY TYPES
# ============================================================================

@fraiseql.type
class Author:
    """Blog author with comprehensive profile information."""

    # Identity
    id: uuid.UUID
    identifier: str

    # Profile
    name: str
    email: str
    bio: Optional[str] = None
    avatar_url: Optional[str] = None

    # Stats (populated by database views)
    post_count: int = 0
    published_post_count: int = 0

    # Audit fields
    created_at: datetime
    updated_at: datetime


@fraiseql.type
class Post:
    """Blog post with rich content and metadata."""

    # Identity
    id: uuid.UUID
    identifier: str  # URL slug

    # Content
    title: str
    content: str
    excerpt: Optional[str] = None

    # Publication
    status: str  # draft, published, archived
    published_at: Optional[datetime] = None

    # Relationships
    author_id: uuid.UUID
    author_name: Optional[str] = None  # Denormalized for performance
    tags: List[str] = []

    # Engagement (populated by database)
    view_count: int = 0
    like_count: int = 0
    comment_count: int = 0

    # SEO
    featured_image_url: Optional[str] = None
    meta_description: Optional[str] = None
    reading_time_minutes: Optional[int] = None

    # Audit fields
    created_at: datetime
    updated_at: datetime
    version: int = 1


@fraiseql.type
class Tag:
    """Content tag for categorization."""

    id: uuid.UUID
    identifier: str  # URL slug
    name: str
    description: Optional[str] = None
    color: Optional[str] = None

    # Usage stats
    post_count: int = 0

    # Hierarchy
    parent_id: Optional[uuid.UUID] = None
    parent_name: Optional[str] = None

    created_at: datetime


@fraiseql.type
class Comment:
    """Blog comment with threading support."""

    id: uuid.UUID
    post_id: uuid.UUID

    # Content
    content: str
    author_name: Optional[str] = None
    author_email: Optional[str] = None

    # Threading
    parent_id: Optional[uuid.UUID] = None
    reply_count: int = 0
    thread_depth: int = 0

    # Moderation
    status: str  # pending, approved, spam, deleted
    moderated_by: Optional[uuid.UUID] = None
    moderated_at: Optional[datetime] = None

    created_at: datetime


# ============================================================================
# AGGREGATE TYPES - For dashboard and analytics
# ============================================================================

@fraiseql.type
class BlogStats:
    """Overall blog statistics."""

    total_posts: int
    published_posts: int
    draft_posts: int
    total_authors: int
    total_comments: int
    total_tags: int

    # Engagement
    total_views: int
    total_likes: int

    # Recent activity
    posts_this_month: int
    comments_this_month: int

    last_updated: datetime


@fraiseql.type
class PopularContent:
    """Popular content analytics."""

    most_viewed_posts: List[Post]
    most_liked_posts: List[Post]
    most_commented_posts: List[Post]
    trending_tags: List[Tag]
    active_authors: List[Author]


# ============================================================================
# FILTER AND PAGINATION TYPES
# ============================================================================

@fraiseql.input
class PostFilterInput:
    """Advanced post filtering options."""

    status: Optional[str] = None
    author_identifier: Optional[str] = None
    tag_identifier: Optional[str] = None
    published_after: Optional[datetime] = None
    published_before: Optional[datetime] = None
    min_reading_time: Optional[int] = None
    max_reading_time: Optional[int] = None
    search_query: Optional[str] = None


@fraiseql.input
class PostSortInput:
    """Post sorting options."""

    field: str  # created_at, published_at, view_count, like_count, title
    direction: str = "DESC"  # ASC or DESC


@fraiseql.type
class PaginationInfo:
    """Pagination metadata."""

    total_count: int
    page_size: int
    current_page: int
    total_pages: int
    has_next_page: bool
    has_previous_page: bool


@fraiseql.type
class PostConnection:
    """Paginated post results."""

    posts: List[Post]
    pagination: PaginationInfo


# ============================================================================
# SEARCH TYPES - Full-text search capabilities
# ============================================================================

@fraiseql.type
class SearchHighlight:
    """Search result highlighting."""

    field: str
    fragments: List[str]


@fraiseql.type
class SearchResult:
    """Search result with relevance."""

    post: Post
    relevance_score: float
    highlights: List[SearchHighlight]
    matched_fields: List[str]


@fraiseql.type
class SearchResponse:
    """Complete search response."""

    results: List[SearchResult]
    total_results: int
    search_time_ms: int
    suggestions: List[str]
    facets: dict  # Category counts, tag counts, etc.
