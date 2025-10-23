"""GraphQL Benchmark Models - FraiseQL Implementation

This module defines the GraphQL schema for the benchmark following FraiseQL's
CQRS architecture with denormalized read models and docstring-based field descriptions.
"""

from datetime import datetime
from typing import Optional
from uuid import UUID

import fraiseql

# ==============================================================================
# ENUMS
# ==============================================================================


@fraiseql.enum
class Direction:
    """Sort direction for ordering results."""

    ASC = "ASC"
    DESC = "DESC"


# ==============================================================================
# TYPES
# ==============================================================================


@fraiseql.type
class User:
    """User account information.

    Fields:
        id: Unique identifier for the user
        name: User's display name
        email: User's email address
        age: User's age in years
        city: City where the user is located
        createdAt: When the user account was created
        posts: Posts authored by this user (denormalized)
    """

    id: UUID
    name: str
    email: str
    age: Optional[int]
    city: Optional[str]
    created_at: datetime
    posts: list["Post"] = []


@fraiseql.type
class Post:
    """Blog post with content and metadata.

    Fields:
        id: Unique identifier for the post
        title: Post title
        content: Full post content
        published: Whether the post is published or draft
        authorId: ID of the user who authored the post
        author: Author information (denormalized)
        comments: Comments on this post (denormalized)
        createdAt: When the post was created
    """

    id: UUID
    title: str
    content: Optional[str]
    published: bool
    author_id: UUID
    author: User
    comments: list["Comment"] = []
    created_at: datetime


@fraiseql.type
class Comment:
    """Comment on a blog post.

    Fields:
        id: Unique identifier for the comment
        content: Comment text content
        postId: ID of the post this comment belongs to
        post: Post information (denormalized)
        authorId: ID of the user who wrote the comment
        author: Author information (denormalized)
        createdAt: When the comment was created
    """

    id: UUID
    content: str
    post_id: UUID
    post: Post
    author_id: UUID
    author: User
    created_at: datetime


# ==============================================================================
# INPUT TYPES
# ==============================================================================


@fraiseql.input
class UserFilter:
    """Filters for querying users.

    Args:
        age_gt: Filter users older than this age
        age_lt: Filter users younger than this age
        city: Filter users by city
        name_contains: Filter users whose name contains this string
    """

    age_gt: Optional[int] = None
    age_lt: Optional[int] = None
    city: Optional[str] = None
    name_contains: Optional[str] = None


@fraiseql.input
class OrderBy:
    """Ordering options for query results.

    Args:
        field: Field name to order by
        direction: Sort direction (ASC or DESC)
    """

    field: str
    direction: Direction = Direction.DESC


@fraiseql.input
class CreateUserInput:
    """Input for creating a new user.

    Args:
        name: User's display name
        email: User's email address
        age: User's age in years
        city: City where the user is located
    """

    name: str
    email: str
    age: Optional[int] = None
    city: Optional[str] = None


@fraiseql.input
class UpdateUserInput:
    """Input for updating an existing user.

    Args:
        name: New display name
        email: New email address
        age: New age
        city: New city
    """

    name: Optional[str] = None
    email: Optional[str] = None
    age: Optional[int] = None
    city: Optional[str] = None


@fraiseql.input
class CreatePostInput:
    """Input for creating a new post.

    Args:
        title: Post title
        content: Post content
        published: Whether to publish immediately
        authorId: ID of the post author
    """

    title: str
    content: Optional[str] = None
    published: bool = False
    author_id: UUID


# ==============================================================================
# RESULT TYPES
# ==============================================================================


@fraiseql.type
class CreateUserResult:
    """Result of creating a new user.

    Fields:
        success: Whether the operation succeeded
        user: The created user (if successful)
        message: Human-readable result message
    """

    success: bool
    user: Optional[User] = None
    message: str


@fraiseql.type
class UpdateUserResult:
    """Result of updating a user.

    Fields:
        success: Whether the operation succeeded
        user: The updated user (if successful)
        message: Human-readable result message
    """

    success: bool
    user: Optional[User] = None
    message: str


@fraiseql.type
class DeleteUserResult:
    """Result of deleting a user.

    Fields:
        success: Whether the operation succeeded
        message: Human-readable result message
    """

    success: bool
    message: str


@fraiseql.type
class CreatePostResult:
    """Result of creating a new post.

    Fields:
        success: Whether the operation succeeded
        post: The created post (if successful)
        message: Human-readable result message
    """

    success: bool
    post: Optional[Post] = None
    message: str
