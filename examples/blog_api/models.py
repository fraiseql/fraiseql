"""Example blog API models using FraiseQL."""

from datetime import datetime
from uuid import UUID

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
