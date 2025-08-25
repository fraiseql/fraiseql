"""Blog API models demonstrating audit patterns."""

from datetime import datetime
from typing import Annotated, Any, Optional
from uuid import UUID

from pydantic import Field

import fraiseql
from fraiseql import fraise_field


@fraiseql.type
class User:
    """User type for blog application."""

    id: UUID
    email: str = fraise_field(description="Email address")
    name: str = fraise_field(description="Display name")
    bio: str | None = fraise_field(description="User biography")
    avatar_url: str | None = fraise_field(description="Profile picture URL")
    created_at: datetime
    updated_at: datetime
    is_active: bool = fraise_field(default=True)
    roles: list[str] = fraise_field(default_factory=list)


@fraiseql.type
class Post:
    """Blog post type."""

    id: UUID
    title: str = fraise_field(description="Post title")
    slug: str = fraise_field(description="URL-friendly identifier")
    content: str = fraise_field(description="Post content in Markdown")
    excerpt: str | None = fraise_field(description="Short description")
    author_id: UUID
    published_at: datetime | None = None
    created_at: datetime
    updated_at: datetime
    tags: list[str] = fraise_field(default_factory=list)
    is_published: bool = fraise_field(default=False)
    view_count: int = fraise_field(default=0)


@fraiseql.type
class Comment:
    """Comment on a blog post."""

    id: UUID
    post_id: UUID
    author_id: UUID
    content: str = fraise_field(description="Comment text")
    created_at: datetime
    updated_at: datetime
    is_approved: bool = fraise_field(default=True)
    parent_comment_id: UUID | None = None  # For nested comments


# Enterprise Pattern Types


@fraiseql.type
class AuditTrail:
    """Comprehensive audit information."""

    created_at: datetime
    created_by_name: Optional[str] = None
    updated_at: Optional[datetime] = None
    updated_by_name: Optional[str] = None
    version: int
    change_reason: Optional[str] = None
    updated_fields: Optional[list[str]] = None


# Enhanced types with audit trails (enterprise pattern examples)


@fraiseql.type
class PostEnterprise:
    """Blog post with audit trail - enterprise pattern example."""

    id: UUID
    title: str
    content: str
    is_published: bool

    # Enterprise features
    audit_trail: AuditTrail
    identifier: Optional[str] = None  # Business identifier
    slug: str = fraise_field(description="URL-friendly identifier")
    excerpt: str | None = fraise_field(description="Short description")
    author_id: UUID
    published_at: datetime | None = None
    tags: list[str] = fraise_field(default_factory=list)
    view_count: int = fraise_field(default=0)


@fraiseql.type
class UserEnterprise:
    """User with comprehensive audit trail."""

    id: UUID
    email: str
    name: str
    bio: str | None = None
    avatar_url: str | None = None
    is_active: bool = True
    roles: list[str] = fraise_field(default_factory=list)

    # Enterprise audit features
    audit_trail: AuditTrail
    identifier: Optional[str] = None  # Business identifier


# Input types for mutations


@fraiseql.input
class CreateUserInput:
    """Input for creating a new user."""

    email: str
    name: str
    password: str
    bio: str | None = None
    avatar_url: str | None = None


@fraiseql.input
class CreateUserInputEnterprise:
    """Post creation input with validation - enterprise pattern example."""

    email: Annotated[str, Field(pattern=r"^[^@]+@[^@]+\.[^@]+$")]
    name: Annotated[str, Field(min_length=2, max_length=100)]
    password: Annotated[str, Field(min_length=8)]
    bio: Annotated[Optional[str], Field(max_length=500)] = None
    avatar_url: Optional[str] = None

    # Audit metadata
    _change_reason: Optional[str] = None
    _expected_version: Optional[int] = None


@fraiseql.input
class UpdateUserInput:
    """Input for updating user profile."""

    name: str | None = None
    bio: str | None = None
    avatar_url: str | None = None
    is_active: bool | None = None


@fraiseql.input
class CreatePostInput:
    """Input for creating a new post."""

    title: str
    content: str
    excerpt: str | None = None
    tags: list[str] | None = None
    is_published: bool = False


@fraiseql.input
class CreatePostInputEnterprise:
    """Post creation input with validation - enterprise pattern example."""

    title: Annotated[str, Field(min_length=3, max_length=200)]
    content: Annotated[str, Field(min_length=50)]
    is_published: bool = False
    excerpt: Annotated[Optional[str], Field(max_length=300)] = None
    tags: Optional[list[str]] = None

    # Audit metadata
    _change_reason: Optional[str] = None
    _expected_version: Optional[int] = None


@fraiseql.input
class UpdatePostInput:
    """Input for updating a post."""

    title: str | None = None
    content: str | None = None
    excerpt: str | None = None
    tags: list[str] | None = None
    is_published: bool | None = None


@fraiseql.input
class CreateCommentInput:
    """Input for creating a comment."""

    post_id: UUID
    content: str
    parent_comment_id: UUID | None = None


@fraiseql.input
class PostFilters:
    """Filters for querying posts."""

    author_id: UUID | None = None
    is_published: bool | None = None
    tags_contain: list[str] | None = None
    created_after: datetime | None = None
    created_before: datetime | None = None
    search: str | None = None  # Search in title and content


@fraiseql.input
class PostOrderBy:
    """Ordering options for posts."""

    field: str  # created_at, updated_at, title, view_count
    direction: str = "desc"  # asc or desc


# Result types for mutations


@fraiseql.success
class CreateUserSuccess:
    """Successful user creation result."""

    user: User
    message: str = "User created successfully"


@fraiseql.failure
class CreateUserError:
    """Failed user creation result."""

    message: str
    code: str
    field_errors: dict[str, str] | None = None


@fraiseql.success
class CreatePostSuccess:
    """Successful post creation result."""

    post: Post
    message: str = "Post created successfully"


@fraiseql.failure
class CreatePostError:
    """Failed post creation result."""

    message: str
    code: str
    field_errors: dict[str, str] | None = None


@fraiseql.success
class UpdatePostSuccess:
    """Successful post update result."""

    post: Post
    message: str = "Post updated successfully"
    updated_fields: list[str]


@fraiseql.failure
class UpdatePostError:
    """Failed post update result."""

    message: str
    code: str


# Enterprise NOOP Result Types


@fraiseql.success
class CreateUserNoop:
    """User creation was a no-op."""

    existing_user: User
    message: str
    noop_reason: str
    was_noop: bool = True


@fraiseql.success
class CreatePostNoop:
    """Post creation was a no-op."""

    existing_post: Post
    message: str
    noop_reason: str
    was_noop: bool = True


@fraiseql.success
class UpdatePostNoop:
    """Post update was a no-op."""

    post: Post
    message: str = "No changes detected"
    noop_reason: str = "no_changes"
    was_noop: bool = True


# Enhanced Success Types with Audit Information


@fraiseql.success
class CreateUserSuccessEnterprise:
    """User created successfully with audit trail."""

    user: UserEnterprise
    message: str = "User created successfully"
    was_noop: bool = False
    audit_metadata: Optional[dict[str, Any]] = None


@fraiseql.success
class CreatePostSuccessEnterprise:
    """Post created successfully with audit trail."""

    post: PostEnterprise
    message: str = "Post created successfully"
    was_noop: bool = False
    generated_slug: Optional[str] = None
    audit_metadata: Optional[dict[str, Any]] = None


@fraiseql.success
class UpdatePostSuccessEnterprise:
    """Post updated successfully with change tracking."""

    post: PostEnterprise
    message: str = "Post updated successfully"
    updated_fields: list[str]
    previous_version: int
    new_version: int
    audit_metadata: Optional[dict[str, Any]] = None


# Enhanced Error Types with Validation Context


@fraiseql.failure
class CreateUserErrorEnterprise:
    """User creation failed with detailed context."""

    message: str
    error_code: str
    field_errors: Optional[dict[str, str]] = None
    validation_context: Optional[dict[str, Any]] = None
    conflicting_user: Optional[User] = None


@fraiseql.failure
class CreatePostErrorEnterprise:
    """Post creation failed with detailed context."""

    message: str
    error_code: str
    field_errors: Optional[dict[str, str]] = None
    validation_context: Optional[dict[str, Any]] = None
    conflicting_post: Optional[Post] = None
