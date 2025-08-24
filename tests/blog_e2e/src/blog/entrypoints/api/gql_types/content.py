"""Content-related GraphQL types for Blog Demo Application.

Following PrintOptim Backend patterns for comprehensive GraphQL type definitions
with proper input/output types and mutation classes.
"""

import uuid
from typing import Optional, List
from datetime import datetime

import fraiseql
from fraiseql import FraiseQLError, UNSET

from ..base_mutation import BlogCreateMutation, BlogUpdateMutation, BlogDeleteMutation
from .common_types.common_outputs import MutationResultBase


# ============================================================================
# INPUT TYPES
# ============================================================================

@fraiseql.input
class CreatePostInput:
    """Input for creating a new blog post."""

    identifier: str
    title: str
    content: str
    excerpt: Optional[str] = UNSET
    author_identifier: str
    tag_identifiers: List[str] = []
    status: str = "draft"
    featured_image_url: Optional[str] = UNSET
    meta_title: Optional[str] = UNSET
    meta_description: Optional[str] = UNSET


@fraiseql.input
class UpdatePostInput:
    """Input for updating an existing blog post."""

    title: Optional[str] = UNSET
    content: Optional[str] = UNSET
    excerpt: Optional[str] = UNSET
    tag_identifiers: Optional[List[str]] = UNSET
    featured_image_url: Optional[str] = UNSET
    meta_title: Optional[str] = UNSET
    meta_description: Optional[str] = UNSET


@fraiseql.input
class PublishPostInput:
    """Input for publishing a blog post."""

    post_identifier: str
    publish_at: Optional[datetime] = UNSET


@fraiseql.input
class DeletePostInput:
    """Input for deleting a blog post."""

    post_identifier: str
    soft_delete: bool = True


# ============================================================================
# OUTPUT TYPES - Entity
# ============================================================================

@fraiseql.type
class PostMetadata:
    """Blog post metadata."""

    meta_title: Optional[str] = None
    meta_description: Optional[str] = None
    featured_image_url: Optional[str] = None
    reading_time_minutes: Optional[int] = None
    view_count: int = 0
    like_count: int = 0
    share_count: int = 0


@fraiseql.type
class Post:
    """Blog post entity for GraphQL responses."""

    # Identity
    id: uuid.UUID
    identifier: str

    # Content
    title: str
    content: str
    excerpt: Optional[str] = None

    # Publication
    status: str
    published_at: Optional[datetime] = None

    # Relationships
    author_id: uuid.UUID
    author_identifier: Optional[str] = None
    author_name: Optional[str] = None
    tags: List[str] = []

    # Metadata
    metadata: Optional[PostMetadata] = None

    # Audit fields
    created_at: datetime
    created_by: Optional[uuid.UUID] = None
    updated_at: datetime
    updated_by: Optional[uuid.UUID] = None
    version: int = 1


# ============================================================================
# SUCCESS/ERROR TYPES
# ============================================================================

class CreatePostSuccess(MutationResultBase):
    """Success response for post creation."""

    post: Optional[Post] = None
    message: str = "Post created successfully"
    errors: List[FraiseQLError] = []


class CreatePostError(MutationResultBase):
    """Error response for post creation."""

    message: str
    errors: List[FraiseQLError] = []
    error_code: Optional[str] = None

    # Specific error context
    conflict_post: Optional[Post] = None
    missing_author: Optional[dict] = None
    invalid_tags: List[str] = []
    field_errors: Optional[dict[str, str]] = None


class CreatePostNoop(MutationResultBase):
    """NOOP response for post creation."""

    message: str = "Post already exists with identical content"
    reason: str = "noop:duplicate_ignored"
    existing_post: Optional[Post] = None


class UpdatePostSuccess(MutationResultBase):
    """Success response for post update."""

    post: Optional[Post] = None
    message: str = "Post updated successfully"
    errors: List[FraiseQLError] = []
    updated_fields: List[str] = []


class UpdatePostError(MutationResultBase):
    """Error response for post update."""

    message: str
    errors: List[FraiseQLError] = []
    error_code: Optional[str] = None

    # Specific error context
    missing_post: Optional[dict] = None
    validation_errors: Optional[dict[str, str]] = None
    version_conflict: Optional[dict] = None


class UpdatePostNoop(MutationResultBase):
    """NOOP response for post update."""

    message: str = "No changes detected in post"
    reason: str = "noop:no_changes"
    current_post: Optional[Post] = None


class PublishPostSuccess(MutationResultBase):
    """Success response for post publication."""

    post: Optional[Post] = None
    message: str = "Post published successfully"
    errors: List[FraiseQLError] = []
    published_at: Optional[datetime] = None


class PublishPostError(MutationResultBase):
    """Error response for post publication."""

    message: str
    errors: List[FraiseQLError] = []
    error_code: Optional[str] = None

    # Specific error context
    publication_requirements: Optional[dict] = None
    missing_post: Optional[dict] = None
    already_published: Optional[dict] = None


class DeletePostSuccess(MutationResultBase):
    """Success response for post deletion."""

    message: str = "Post deleted successfully"
    errors: List[FraiseQLError] = []
    deleted_id: Optional[uuid.UUID] = None
    soft_deleted: bool = True


class DeletePostError(MutationResultBase):
    """Error response for post deletion."""

    message: str
    errors: List[FraiseQLError] = []
    error_code: Optional[str] = None

    # Specific error context
    missing_post: Optional[dict] = None
    authorization_failure: bool = False


# ============================================================================
# MUTATION CLASSES
# ============================================================================

class CreatePost(
    BlogCreateMutation,
    function="app.create_post"
):
    """Create a new blog post with comprehensive validation and error handling.

    This mutation demonstrates enterprise patterns:
    - Input validation at GraphQL and database levels
    - Comprehensive error responses with context
    - NOOP handling for idempotent operations
    - Automatic audit trail creation
    """

    input: CreatePostInput
    success: CreatePostSuccess  # Auto-decorated
    failure: CreatePostError    # Auto-decorated
    noop: CreatePostNoop       # Auto-decorated


class UpdatePost(
    BlogUpdateMutation,
    function="app.update_post"
):
    """Update an existing blog post with optimistic locking.

    Features:
    - Partial update support (only provided fields updated)
    - Version-based conflict detection
    - Field-level change tracking
    - NOOP detection for identical updates
    """

    input: UpdatePostInput
    success: UpdatePostSuccess  # Auto-decorated
    failure: UpdatePostError    # Auto-decorated
    noop: UpdatePostNoop       # Auto-decorated


class PublishPost(
    BlogUpdateMutation,
    function="app.publish_post"
):
    """Publish a blog post with publication requirements validation.

    Business rules:
    - Post must have title, content, and author
    - Content must meet minimum length requirements
    - Post must be in draft status
    - Publication date can be scheduled for future
    """

    input: PublishPostInput
    success: PublishPostSuccess  # Auto-decorated
    failure: PublishPostError    # Auto-decorated


class DeletePost(
    BlogDeleteMutation,
    function="app.delete_post"
):
    """Delete a blog post with soft delete support.

    Features:
    - Soft delete by default (preserves data)
    - Hard delete option for permanent removal
    - Authorization checking
    - Cascade handling for comments and tags
    """

    input: DeletePostInput
    success: DeletePostSuccess  # Auto-decorated
    failure: DeletePostError    # Auto-decorated


# ============================================================================
# QUERY RESOLVERS (if needed for standalone queries)
# ============================================================================

@fraiseql.query
async def posts(
    info,
    limit: int = 10,
    offset: int = 0,
    status: Optional[str] = None,
    author_identifier: Optional[str] = None,
    tag_identifier: Optional[str] = None
) -> List[Post]:
    """Query blog posts with filtering and pagination."""
    repo = info.context["db"]

    # Build filter conditions
    filters = {}
    if status:
        filters["status"] = status
    if author_identifier:
        filters["author_identifier"] = author_identifier
    if tag_identifier:
        filters["tag_identifier"] = tag_identifier

    # Use materialized view for performance
    results = await repo.find(
        "tv_post",
        limit=limit,
        offset=offset,
        **filters
    )

    return [Post.from_dict(result) for result in results]


@fraiseql.query
async def post(info, identifier: str) -> Optional[Post]:
    """Get a single blog post by identifier."""
    repo = info.context["db"]

    result = await repo.find_one("v_post", identifier=identifier)

    return Post.from_dict(result) if result else None
