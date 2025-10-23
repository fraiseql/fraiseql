"""Blog domain models with Trinity Identifiers for FraiseQL example.

This module demonstrates FraiseQL with Trinity pattern:
- Three-tier ID system (pk_*, id, identifier)
- Type definitions with Trinity support
- SERIAL foreign keys for faster joins
- GraphQL exposes only id and identifier (not pk_*)
"""

from datetime import datetime
from enum import Enum
from typing import Any
from uuid import UUID

import fraiseql
from fraiseql.patterns import TrinityMixin, get_pk_column_name
from graphql import GraphQLResolveInfo


# Domain enums
@fraiseql.enum
class UserRole(str, Enum):
    """User roles in the blog system."""

    ADMIN = "admin"
    AUTHOR = "author"
    USER = "user"


@fraiseql.enum
class PostStatus(str, Enum):
    """Post publication status."""

    DRAFT = "draft"
    PUBLISHED = "published"
    ARCHIVED = "archived"


@fraiseql.enum
class CommentStatus(str, Enum):
    """Comment moderation status."""

    PENDING = "pending"
    APPROVED = "approved"
    REJECTED = "rejected"


# Core domain types with Trinity
@fraiseql.type(sql_source="users", jsonb_column=None)
class User(TrinityMixin):
    """User with profile and Trinity identifiers.

    Trinity IDs:
    - pk_user (SERIAL): Internal, fast joins (not exposed)
    - id (UUID): Public API, secure
    - identifier (TEXT): URL slug (@username)
    """

    # Public Trinity IDs (exposed in GraphQL)
    id: UUID
    identifier: str | None  # @username

    # User data
    username: str
    email: str
    role: UserRole
    created_at: datetime
    updated_at: datetime
    profile_data: dict[str, Any] | None

    @fraiseql.field
    async def full_name(self, info: GraphQLResolveInfo) -> str | None:
        """Full name from profile data."""
        if self.profile_data:
            first = self.profile_data.get("first_name", "")
            last = self.profile_data.get("last_name", "")
            if first or last:
                return f"{first} {last}".strip()
        return None

    @fraiseql.field
    async def url(self, info: GraphQLResolveInfo) -> str:
        """User profile URL."""
        return f"/users/{self.identifier}" if self.identifier else f"/users/{self.id}"


@fraiseql.type(sql_source="posts", jsonb_column=None)
class Post(TrinityMixin):
    """Blog post with Trinity identifiers.

    Trinity IDs:
    - pk_post (SERIAL): Internal, fast joins (not exposed)
    - id (UUID): Public API, secure
    - identifier (TEXT): URL slug (post-title)

    Foreign Keys:
    - fk_author (SERIAL): Fast join to users
    """

    # Public Trinity IDs (exposed in GraphQL)
    id: UUID
    identifier: str | None  # post-slug

    # Post data
    title: str
    slug: str
    content: str
    excerpt: str | None
    status: PostStatus
    published_at: datetime | None
    created_at: datetime
    updated_at: datetime

    # Internal FK (not exposed - use fk_author internally)
    # fk_author: int

    @fraiseql.field
    async def author(self, info: GraphQLResolveInfo) -> User:
        """Post author via SERIAL FK."""
        db = info.context["db"]
        # Use SERIAL FK for fast join
        fk_author = getattr(self, "fk_author")
        return await db.find_one("users", pk_user=fk_author)

    @fraiseql.field
    async def url(self, info: GraphQLResolveInfo) -> str:
        """Post URL."""
        return f"/posts/{self.identifier}" if self.identifier else f"/posts/{self.id}"

    @fraiseql.field
    async def comment_count(self, info: GraphQLResolveInfo) -> int:
        """Number of approved comments."""
        db = info.context["db"]
        from fraiseql.db import DatabaseQuery

        # Use SERIAL FK for fast join
        pk_post = self.get_internal_pk()
        query = DatabaseQuery(
            "SELECT COUNT(*) as count FROM comments WHERE pk_post = %s AND status = %s",
            [pk_post, CommentStatus.APPROVED.value],
        )
        result = await db.run(query)
        return result[0]["count"] if result else 0


@fraiseql.type(sql_source="comments", jsonb_column=None)
class Comment(TrinityMixin):
    """Comment with Trinity identifiers.

    Trinity IDs:
    - pk_comment (SERIAL): Internal, fast joins (not exposed)
    - id (UUID): Public API, secure

    Foreign Keys:
    - fk_post (SERIAL): Fast join to posts
    - fk_author (SERIAL): Fast join to users
    - fk_parent (SERIAL): Fast join to parent comment
    """

    # Public Trinity IDs (exposed in GraphQL)
    id: UUID

    # Comment data
    content: str
    status: CommentStatus
    created_at: datetime
    updated_at: datetime

    # Internal FKs (not exposed - use pk_* internally)
    # fk_post: int
    # fk_author: int
    # fk_parent: int | None

    @fraiseql.field
    async def author(self, info: GraphQLResolveInfo) -> User:
        """Comment author via SERIAL FK."""
        db = info.context["db"]
        fk_author = getattr(self, "fk_author")
        return await db.find_one("users", pk_user=fk_author)

    @fraiseql.field
    async def post(self, info: GraphQLResolveInfo) -> Post:
        """Comment's post via SERIAL FK."""
        db = info.context["db"]
        fk_post = getattr(self, "fk_post")
        return await db.find_one("posts", pk_post=fk_post)


@fraiseql.type(sql_source="tags", jsonb_column=None)
class Tag(TrinityMixin):
    """Content tag with Trinity identifiers.

    Trinity IDs:
    - pk_tag (SERIAL): Internal, fast joins (not exposed)
    - id (UUID): Public API, secure
    - identifier (TEXT): URL slug (tag-name)
    """

    # Public Trinity IDs (exposed in GraphQL)
    id: UUID
    identifier: str | None  # tag-slug

    # Tag data
    name: str
    slug: str
    color: str | None
    description: str | None

    @fraiseql.field
    async def url(self, info: GraphQLResolveInfo) -> str:
        """Tag URL."""
        return f"/tags/{self.identifier}" if self.identifier else f"/tags/{self.id}"

    @fraiseql.field
    async def post_count(self, info: GraphQLResolveInfo) -> int:
        """Number of published posts with this tag."""
        db = info.context["db"]
        from fraiseql.db import DatabaseQuery

        # Use SERIAL FK for fast join
        pk_tag = self.get_internal_pk()
        query = DatabaseQuery(
            """
            SELECT COUNT(*) as count FROM posts p
            JOIN post_tags pt ON p.pk_post = pt.pk_post
            WHERE pt.pk_tag = %s AND p.status = %s
        """,
            [pk_tag, PostStatus.PUBLISHED.value],
        )
        result = await db.run(query)
        return result[0]["count"] if result else 0


# Input types
@fraiseql.input
class CreatePostInput:
    """Input for creating a blog post."""

    title: str
    content: str
    excerpt: str | None = None
    tag_ids: list[UUID] | None = None


@fraiseql.input
class UpdatePostInput:
    """Input for updating a blog post."""

    title: str | None = None
    content: str | None = None
    excerpt: str | None = None
    status: PostStatus | None = None
    tag_ids: list[UUID] | None = None


@fraiseql.input
class CreateCommentInput:
    """Input for creating a comment."""

    post_id: UUID  # Public UUID (will be converted to fk_post)
    content: str
    parent_id: UUID | None = None  # Public UUID (will be converted to fk_parent)


@fraiseql.input
class CreateTagInput:
    """Input for creating a tag."""

    name: str
    color: str | None = "#6366f1"
    description: str | None = None


@fraiseql.input
class CreateUserInput:
    """Input for creating a user."""

    username: str
    email: str
    password: str
    role: UserRole = UserRole.USER
    profile_data: dict[str, Any] | None = None


# Success result types
@fraiseql.success
class CreatePostSuccess:
    """Success response for post creation."""

    post: Post
    message: str = "Post created successfully"


@fraiseql.success
class UpdatePostSuccess:
    """Success response for post update."""

    post: Post
    message: str = "Post updated successfully"


@fraiseql.success
class CreateCommentSuccess:
    """Success response for comment creation."""

    comment: Comment
    message: str = "Comment created successfully"


@fraiseql.success
class CreateTagSuccess:
    """Success response for tag creation."""

    tag: Tag
    message: str = "Tag created successfully"


@fraiseql.success
class CreateUserSuccess:
    """Success response for user creation."""

    user: User
    message: str = "User created successfully"


# Error result types
@fraiseql.failure
class ValidationError:
    """Validation error with details."""

    message: str
    code: str = "VALIDATION_ERROR"
    field_errors: list[dict[str, str]] | None = None


@fraiseql.failure
class NotFoundError:
    """Entity not found error."""

    message: str
    code: str = "NOT_FOUND"
    entity_type: str | None = None
    entity_id: UUID | None = None


@fraiseql.failure
class PermissionError:
    """Permission denied error."""

    message: str
    code: str = "PERMISSION_DENIED"
    required_role: str | None = None


# Mutation classes with Trinity support
@fraiseql.mutation
class CreatePost:
    """Create a new blog post."""

    input: CreatePostInput
    success: CreatePostSuccess
    failure: ValidationError | PermissionError

    async def resolve(
        self, info: GraphQLResolveInfo
    ) -> CreatePostSuccess | ValidationError | PermissionError:
        db = info.context["db"]
        user_pk = info.context.get("user_pk")  # Get SERIAL pk, not UUID

        if not user_pk:
            return PermissionError(message="Authentication required")

        try:
            # Generate slug from title
            slug = self.input.title.lower().replace(" ", "-").replace("_", "-")

            # Create post using SERIAL FK
            post_data = {
                "title": self.input.title,
                "slug": slug,
                "content": self.input.content,
                "excerpt": self.input.excerpt or self.input.content[:200],
                "fk_author": user_pk,  # SERIAL FK (fast)
                "status": PostStatus.DRAFT.value,
            }

            # Insert and get pk_post
            from fraiseql.db import DatabaseQuery

            insert_query = DatabaseQuery(
                """
                INSERT INTO posts (title, slug, content, excerpt, fk_author, status)
                VALUES (%s, %s, %s, %s, %s, %s)
                RETURNING pk_post
                """,
                [
                    post_data["title"],
                    post_data["slug"],
                    post_data["content"],
                    post_data["excerpt"],
                    post_data["fk_author"],
                    post_data["status"],
                ],
            )
            result = await db.run(insert_query)
            pk_post = result[0]["pk_post"]

            # Add tags if provided (using SERIAL FKs)
            if self.input.tag_ids:
                for tag_id in self.input.tag_ids:
                    # Convert UUID to pk_tag
                    tag = await db.find_one("tags", id=tag_id)
                    if tag:
                        pk_tag = getattr(tag, "pk_tag")
                        tag_query = DatabaseQuery(
                            "INSERT INTO post_tags (fk_post, fk_tag) VALUES (%s, %s)",
                            [pk_post, pk_tag],
                        )
                        await db.run(tag_query)

            # Return created post
            post = await db.find_one("posts", pk_post=pk_post)
            return CreatePostSuccess(post=post)

        except Exception as e:
            return ValidationError(message=f"Failed to create post: {e!s}")


# Query resolvers
@fraiseql.query
async def posts(
    info: GraphQLResolveInfo,
    status: PostStatus | None = None,
    limit: int = 20,
    offset: int = 0,
) -> list[Post]:
    """Query posts with filtering and pagination."""
    db = info.context["db"]

    # Build query with SERIAL joins (fast)
    where_conditions = []
    params = []

    if status:
        where_conditions.append("status = %s")
        params.append(status.value)

    where_clause = " AND ".join(where_conditions) if where_conditions else "1=1"
    query = f"""
        SELECT * FROM posts
        WHERE {where_clause}
        ORDER BY created_at DESC
        LIMIT %s OFFSET %s
    """
    params.extend([limit, offset])

    from fraiseql.db import DatabaseQuery

    db_query = DatabaseQuery(query, params)
    result = await db.run(db_query)
    return [Post(**row) for row in result]


@fraiseql.query
async def post(
    info: GraphQLResolveInfo,
    id: UUID | None = None,
    identifier: str | None = None,
) -> Post | None:
    """Get a single post by UUID id or text identifier."""
    db = info.context["db"]

    if id:
        result = await db.find_one("posts", id=id)
    elif identifier:
        result = await db.find_one("posts", identifier=identifier)
    else:
        return None

    return result


@fraiseql.query
async def user(
    info: GraphQLResolveInfo,
    id: UUID | None = None,
    identifier: str | None = None,
) -> User | None:
    """Get a single user by UUID id or text identifier."""
    db = info.context["db"]

    if id:
        result = await db.find_one("users", id=id)
    elif identifier:
        result = await db.find_one("users", identifier=identifier)
    else:
        return None

    return result


@fraiseql.query
async def tag(
    info: GraphQLResolveInfo,
    id: UUID | None = None,
    identifier: str | None = None,
) -> Tag | None:
    """Get a single tag by UUID id or text identifier."""
    db = info.context["db"]

    if id:
        result = await db.find_one("tags", id=id)
    elif identifier:
        result = await db.find_one("tags", identifier=identifier)
    else:
        return None

    return result


@fraiseql.query
async def tags(info: GraphQLResolveInfo, limit: int = 50) -> list[Tag]:
    """Get all tags."""
    db = info.context["db"]
    result = await db.find("tags", limit=limit, order_by="name ASC")
    return result


@fraiseql.query
async def users(info: GraphQLResolveInfo, limit: int = 20) -> list[User]:
    """Get users (admin only)."""
    db = info.context["db"]
    result = await db.find("users", limit=limit, order_by="created_at DESC")
    return result


# Export collections for app registration
BLOG_TYPES = [User, Post, Comment, Tag, UserRole, PostStatus, CommentStatus]
BLOG_MUTATIONS = [CreatePost]
BLOG_QUERIES = [posts, post, user, tag, tags, users]
