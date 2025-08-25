"""GraphQL types and mutations for Blog E2E Test Suite.

GREEN Phase: Minimal FraiseQL implementation following PrintOptim patterns
to make the RED phase tests pass.

This module implements:
- Input/Output types using FraiseQL decorators
- PrintOptimMutation pattern for consistent error handling
- Database-first architecture with rich error responses
"""

import uuid
from typing import Any

import fraiseql
from fraiseql import UNSET


# ============================================================================
# BASE TYPES - Common output structures
# ============================================================================

@fraiseql.type
class MutationResultBase:
    """Base class for mutation results following PrintOptim patterns."""

    message: str | None = None
    original_payload: dict[str, Any] | None = None


# ============================================================================
# AUTHOR TYPES - Content creators
# ============================================================================

@fraiseql.input
class CreateAuthorInput:
    """Input for creating a new author."""

    identifier: str
    name: str
    email: str
    bio: str | None = UNSET
    avatar_url: str | None = UNSET
    social_links: dict[str, Any] | None = UNSET


@fraiseql.type
class Author:
    """Author entity for GraphQL responses."""

    id: uuid.UUID
    identifier: str
    name: str
    email: str
    bio: str | None = None
    avatar_url: str | None = None
    post_count: int = 0
    last_post_at: str | None = None  # ISO datetime string
    created_at: str  # ISO datetime string
    updated_at: str  # ISO datetime string


@fraiseql.success
class CreateAuthorSuccess(MutationResultBase):
    """Success response for author creation."""

    author: Author | None = None
    message: str = "Author created successfully"


@fraiseql.failure
class CreateAuthorError(MutationResultBase):
    """Error response for author creation."""

    message: str
    error_code: str
    conflict_author: Author | None = None
    original_payload: dict[str, Any] | None = None


# ============================================================================
# POST TYPES - Blog content
# ============================================================================

@fraiseql.input
class CreatePostInput:
    """Input for creating a new blog post."""

    identifier: str
    title: str
    content: str
    excerpt: str | None = UNSET
    featured_image_url: str | None = UNSET
    author_identifier: str
    tag_identifiers: list[str] | None = UNSET
    status: str = "draft"  # draft, published, archived
    publish_at: str | None = UNSET  # ISO datetime string


@fraiseql.type
class Post:
    """Post entity for GraphQL responses."""

    id: uuid.UUID
    identifier: str
    title: str
    content: str
    excerpt: str | None = None
    featured_image_url: str | None = None
    author_id: uuid.UUID
    author_name: str | None = None
    status: str
    published_at: str | None = None  # ISO datetime string
    tags: list[dict[str, Any]] | None = None
    comment_count: int = 0
    tag_count: int = 0
    created_at: str  # ISO datetime string
    updated_at: str  # ISO datetime string


@fraiseql.success
class CreatePostSuccess(MutationResultBase):
    """Success response for post creation."""

    post: Post | None = None
    message: str = "Post created successfully"


@fraiseql.failure
class CreatePostError(MutationResultBase):
    """Error response for post creation."""

    message: str
    error_code: str
    conflict_post: Post | None = None
    missing_author: dict[str, str] | None = None  # {"identifier": "author-id"}
    invalid_tags: list[str] | None = None
    original_payload: dict[str, Any] | None = None


# ============================================================================
# TAG TYPES - Content categorization
# ============================================================================

@fraiseql.input
class CreateTagInput:
    """Input for creating a new tag."""

    identifier: str
    name: str
    description: str | None = UNSET
    color: str | None = UNSET
    parent_identifier: str | None = UNSET


@fraiseql.type
class Tag:
    """Tag entity for GraphQL responses."""

    id: uuid.UUID
    identifier: str
    name: str
    description: str | None = None
    color: str | None = None
    parent_id: uuid.UUID | None = None
    usage_count: int = 0
    post_count: int = 0
    children_count: int = 0
    created_at: str  # ISO datetime string
    updated_at: str  # ISO datetime string


@fraiseql.success
class CreateTagSuccess(MutationResultBase):
    """Success response for tag creation."""

    tag: Tag | None = None
    message: str = "Tag created successfully"


@fraiseql.failure
class CreateTagError(MutationResultBase):
    """Error response for tag creation."""

    message: str
    error_code: str
    conflict_tag: Tag | None = None
    missing_parent: dict[str, str] | None = None
    original_payload: dict[str, Any] | None = None


# ============================================================================
# COMMENT TYPES - User interactions
# ============================================================================

@fraiseql.input
class CreateCommentInput:
    """Input for creating a new comment."""

    post_identifier: str
    parent_comment_id: uuid.UUID | None = UNSET
    author_identifier: str | None = UNSET
    content: str
    author_name: str | None = UNSET  # For anonymous comments
    author_email: str | None = UNSET  # For anonymous comments


@fraiseql.type
class Comment:
    """Comment entity for GraphQL responses."""

    id: uuid.UUID
    content: str
    post_id: uuid.UUID
    parent_id: uuid.UUID | None = None
    author_id: uuid.UUID | None = None
    author_name: str | None = None
    status: str = "pending"  # pending, approved, spam, deleted
    reply_count: int = 0
    thread_depth: int = 0
    created_at: str  # ISO datetime string
    updated_at: str  # ISO datetime string


@fraiseql.success
class CreateCommentSuccess(MutationResultBase):
    """Success response for comment creation."""

    comment: Comment | None = None
    message: str = "Comment created successfully"


@fraiseql.failure
class CreateCommentError(MutationResultBase):
    """Error response for comment creation."""

    message: str
    error_code: str
    missing_post: dict[str, str] | None = None  # {"identifier": "post-id"}
    spam_reasons: list[str] | None = None
    original_payload: dict[str, Any] | None = None


# ============================================================================
# PRINTOPTIM MUTATION PATTERN - Following base_mutation.py patterns
# ============================================================================

class BlogMutationBase:
    """Base class for blog mutations with PrintOptim patterns.

    This follows the same pattern as PrintOptimMutation but simplified
    for the E2E test context.
    """

    def __init_subclass__(
        cls,
        function: str,
        schema: str = "app",
        context_params: dict[str, str] | None = None,
        **kwargs: Any,
    ) -> None:
        """Initialize subclass with automatic mutation decorator application."""
        super().__init_subclass__(**kwargs)

        # Validate required type annotations
        if not hasattr(cls, "__annotations__"):
            raise TypeError(
                f"{cls.__name__} must define input, success, and failure type annotations"
            )

        annotations = cls.__annotations__
        required = {"input", "success", "failure"}
        missing = required - set(annotations.keys())

        if missing:
            raise TypeError(
                f"{cls.__name__} missing required type annotations: {', '.join(sorted(missing))}"
            )

        # Apply the FraiseQL mutation decorator with error configuration
        # Using DEFAULT_ERROR_CONFIG to populate errors array automatically
        fraiseql.mutation(
            function=function,
            schema=schema,
            context_params=context_params or {},
            error_config=fraiseql.DEFAULT_ERROR_CONFIG  # Auto-populates error arrays
        )(cls)


# ============================================================================
# MUTATION IMPLEMENTATIONS - Following PrintOptim patterns
# ============================================================================

class CreateAuthor(
    BlogMutationBase,
    function="create_author",
    context_params={"user_id": "input_created_by"}
):
    """Create a new author with comprehensive error handling."""

    input: CreateAuthorInput
    success: CreateAuthorSuccess
    failure: CreateAuthorError


class CreatePost(
    BlogMutationBase,
    function="create_post",
    context_params={"user_id": "input_created_by"}
):
    """Create a new blog post with comprehensive error handling."""

    input: CreatePostInput
    success: CreatePostSuccess
    failure: CreatePostError


class CreateTag(
    BlogMutationBase,
    function="create_tag",
    context_params={"user_id": "input_created_by"}
):
    """Create a new tag with hierarchy support."""

    input: CreateTagInput
    success: CreateTagSuccess
    failure: CreateTagError


class CreateComment(
    BlogMutationBase,
    function="create_comment",
    context_params={"user_id": "input_created_by"}
):
    """Create a new comment with spam detection."""

    input: CreateCommentInput
    success: CreateCommentSuccess
    failure: CreateCommentError


# ============================================================================
# HELPER FUNCTIONS - Response mapping from database results
# ============================================================================

def map_author_from_result(result: dict) -> Author | None:
    """Map database result to Author GraphQL type."""
    if not result or not result.get("object_data"):
        return None

    data = result["object_data"]
    return Author(
        id=result["id"],
        identifier=data.get("identifier", ""),
        name=data.get("name", ""),
        email=data.get("email", ""),
        bio=data.get("bio"),
        avatar_url=data.get("avatar_url"),
        post_count=data.get("post_count", 0),
        last_post_at=data.get("last_post_at"),
        created_at=data.get("created_at", ""),
        updated_at=data.get("updated_at", "")
    )


def map_post_from_result(result: dict) -> Post | None:
    """Map database result to Post GraphQL type."""
    if not result or not result.get("object_data"):
        return None

    data = result["object_data"]
    author = data.get("author", {})

    return Post(
        id=result["id"],
        identifier=data.get("identifier", ""),
        title=data.get("title", ""),
        content=data.get("content", ""),
        excerpt=data.get("excerpt"),
        featured_image_url=data.get("featured_image_url"),
        author_id=author.get("id") if author else result["id"],  # Fallback
        author_name=author.get("name"),
        status=data.get("status", "draft"),
        published_at=data.get("published_at"),
        tags=data.get("tags", []),
        comment_count=data.get("comment_count", 0),
        tag_count=data.get("tag_count", 0),
        created_at=data.get("created_at", ""),
        updated_at=data.get("updated_at", "")
    )


def map_tag_from_result(result: dict) -> Tag | None:
    """Map database result to Tag GraphQL type."""
    if not result or not result.get("object_data"):
        return None

    data = result["object_data"]
    return Tag(
        id=result["id"],
        identifier=data.get("identifier", ""),
        name=data.get("name", ""),
        description=data.get("description"),
        color=data.get("color"),
        parent_id=data.get("parent_id"),
        usage_count=data.get("usage_count", 0),
        post_count=data.get("post_count", 0),
        children_count=data.get("children_count", 0),
        created_at=data.get("created_at", ""),
        updated_at=data.get("updated_at", "")
    )


def map_comment_from_result(result: dict) -> Comment | None:
    """Map database result to Comment GraphQL type."""
    if not result or not result.get("object_data"):
        return None

    data = result["object_data"]
    return Comment(
        id=result["id"],
        content=data.get("content", ""),
        post_id=data.get("post_id", result["id"]),  # Fallback
        parent_id=data.get("parent_id"),
        author_id=data.get("author_id"),
        author_name=data.get("author_name"),
        status=data.get("status", "pending"),
        reply_count=data.get("reply_count", 0),
        thread_depth=data.get("thread_depth", 0),
        created_at=data.get("created_at", ""),
        updated_at=data.get("updated_at", "")
    )
