"""Blog domain models for FraiseQL simple example.

This module demonstrates FraiseQL fundamentals:
- Type definitions with proper annotations
- Relationship modeling with field resolvers
- JSONB field usage for flexible data
- Input validation and mutation patterns
- Error handling with success/failure types
"""

from datetime import datetime
from enum import Enum
from typing import Any
from uuid import UUID

import fraiseql
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


# Core domain types
@fraiseql.type(sql_source="tb_user", jsonb_column=None)
class User:
    """User with profile and authentication."""

    pk_user: int
    id: UUID
    identifier: str
    email: str
    password_hash: str
    role: UserRole
    profile_data: dict[str, Any] | None
    created_at: datetime
    updated_at: datetime

    # @fraiseql.field
    # async def posts(self, info: GraphQLResolveInfo) -> list["Post"]:
    #     """User's posts."""
    #     db = info.context["db"]
    #     return await db.find("posts", author_id=self.id, order_by="created_at DESC")

    @fraiseql.field
    async def full_name(self, info: GraphQLResolveInfo) -> str | None:
        """Full name from profile data."""
        if self.profile_data:
            first = self.profile_data.get("first_name", "")
            last = self.profile_data.get("last_name", "")
            if first or last:
                return f"{first} {last}".strip()
        return None


@fraiseql.type(sql_source="tb_post", jsonb_column=None)
class Post:
    """Blog post with content and metadata."""

    pk_post: int
    id: UUID
    identifier: str
    title: str
    content: str
    excerpt: str | None
    fk_author: int
    status: PostStatus
    published_at: datetime | None
    created_at: datetime
    updated_at: datetime

    @fraiseql.field
    async def author(self, info: GraphQLResolveInfo) -> User | None:
        """Post author."""
        db = info.context["db"]
        from fraiseql.db import DatabaseQuery

        query = DatabaseQuery("SELECT * FROM tb_user WHERE pk_user = %s", [self.fk_author])
        result = await db.run(query)
        if result:
            return User(**result[0])
        return None

    # @fraiseql.field
    # async def tags(self, info: GraphQLResolveInfo) -> list["Tag"]:
    #     """Post tags."""
    #     db = info.context["db"]
    #     # Join through post_tags table
    #     result = await db.execute(
    #         """
    #         SELECT t.* FROM tags t
    #         JOIN post_tags pt ON t.id = pt.tag_id
    #         WHERE pt.post_id = %s
    #     """,
    #         [self.id],
    #     )
    #     return [Tag(**row) for row in result]

    # @fraiseql.field
    # async def comments(self, info: GraphQLResolveInfo) -> list["Comment"]:
    #     """Post comments."""
    #     db = info.context["db"]
    #     return await db.find("comments", post_id=self.id, status=CommentStatus.APPROVED)

    @fraiseql.field
    async def comment_count(self, info: GraphQLResolveInfo) -> int:
        """Number of approved comments."""
        db = info.context["db"]
        from fraiseql.db import DatabaseQuery

        query = DatabaseQuery(
            "SELECT COUNT(*) as count FROM comments WHERE post_id = %s AND status = %s",
            [self.id, CommentStatus.APPROVED],
        )
        result = await db.run(query)
        return result[0]["count"] if result else 0


@fraiseql.type(sql_source="tb_comment", jsonb_column=None)
class Comment:
    """Comment with threading support."""

    pk_comment: int
    id: UUID
    identifier: str | None
    fk_post: int
    fk_author: int
    fk_parent: int | None
    content: str
    status: CommentStatus
    created_at: datetime
    updated_at: datetime

    @fraiseql.field
    async def author(self, info: GraphQLResolveInfo) -> User | None:
        """Comment author."""
        db = info.context["db"]
        from fraiseql.db import DatabaseQuery

        query = DatabaseQuery("SELECT * FROM tb_user WHERE pk_user = %s", [self.fk_author])
        result = await db.run(query)
        if result:
            return User(**result[0])
        return None

    @fraiseql.field
    async def post(self, info: GraphQLResolveInfo) -> Post | None:
        """Comment's post."""
        db = info.context["db"]
        from fraiseql.db import DatabaseQuery

        query = DatabaseQuery("SELECT * FROM tb_post WHERE pk_post = %s", [self.fk_post])
        result = await db.run(query)
        if result:
            return Post(**result[0])
        return None

    @fraiseql.field
    async def post(self, info: GraphQLResolveInfo) -> Post:
        """Comment's post."""
        db = info.context["db"]
        from fraiseql.db import DatabaseQuery

        query = DatabaseQuery("SELECT * FROM tb_post WHERE pk_post = %s", [self.fk_post])
        result = await db.run(query)
        if result:
            return Post(**result[0])
        return None

    # @fraiseql.field
    # async def parent(self, info: GraphQLResolveInfo) -> "Comment" | None:
    #     """Parent comment if reply."""
    #     if not self.parent_id:
    #         return None
    #     db = info.context["db"]
    #     return await db.find_one("comments", id=self.parent_id)

    # @fraiseql.field
    # async def replies(self, info: GraphQLResolveInfo) -> list["Comment"]:
    #     """Replies to this comment."""
    #     db = info.context["db"]
    #     return await db.find("comments", parent_id=self.id, status=CommentStatus.APPROVED)


@fraiseql.type(sql_source="tb_tag", jsonb_column=None)
class Tag:
    """Content tag/category."""

    pk_tag: int
    id: UUID
    identifier: str
    name: str
    color: str | None
    description: str | None
    created_at: datetime

    # @fraiseql.field
    # async def posts(self, info: GraphQLResolveInfo) -> list["Post"]:
    #     """Posts with this tag."""
    #     db = info.context["db"]
    #     result = await db.execute(
    #         """
    #         SELECT p.* FROM posts p
    #         JOIN post_tags pt ON p.id = pt.post_id
    #         WHERE pt.tag_id = %s AND p.status = %s
    #         ORDER BY p.created_at DESC
    #     """,
    #         [self.id, PostStatus.PUBLISHED],
    #     )
    #     return [Post(**row) for row in result]

    @fraiseql.field
    async def post_count(self, info: GraphQLResolveInfo) -> int:
        """Number of published posts with this tag."""
        db = info.context["db"]
        from fraiseql.db import DatabaseQuery

        query = DatabaseQuery(
            """
            SELECT COUNT(*) as count FROM posts p
            JOIN post_tags pt ON p.id = pt.post_id
            WHERE pt.tag_id = %s AND p.status = %s
        """,
            [self.id, PostStatus.PUBLISHED],
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

    post_id: UUID
    content: str
    parent_id: UUID | None = None


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


# Filter inputs
@fraiseql.input
class PostWhereInput:
    """Filter posts by various criteria."""

    status: PostStatus | None = None
    author_id: UUID | None = None
    title_contains: str | None = None
    tag_ids: list[UUID] | None = None


@fraiseql.input
class PostOrderByInput:
    """Order posts by field and direction."""

    field: str = "created_at"
    direction: str = "DESC"


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


@fraiseql.failure
class BlogError:
    """Generic blog operation error."""

    message: str
    code: str
    entity_type: str | None = None
    entity_id: UUID | None = None
    field_errors: list[dict[str, str]] | None = None
    required_role: str | None = None


# Mutation classes (following printoptim pattern: database functions handle validation)
@fraiseql.mutation
class CreatePost:
    """Create a new blog post."""

    input: CreatePostInput
    success: CreatePostSuccess
    failure: BlogError

    async def resolve(
        self, info: GraphQLResolveInfo
    ) -> CreatePostSuccess | BlogError:
        db = info.context["db"]
        user_id = info.context["user_id"]

        try:
            # Generate slug from title
            slug = self.input.title.lower().replace(" ", "-").replace("_", "-")

            # Create post
            post_data = {
                "title": self.input.title,
                "slug": slug,
                "content": self.input.content,
                "excerpt": self.input.excerpt or self.input.content[:200],
                "author_id": user_id,
                "status": PostStatus.DRAFT,
            }

            post_id = await db.insert("posts", post_data, returning="id")

            # Add tags if provided
            if self.input.tag_ids:
                for tag_id in self.input.tag_ids:
                    await db.insert("post_tags", {"post_id": post_id, "tag_id": tag_id})

            # Return created post
            post = await db.find_one("posts", id=post_id)
            return CreatePostSuccess(post=Post(**post))

        except Exception as e:
            return BlogError(message=f"Failed to create post: {e!s}", code="VALIDATION_ERROR")


@fraiseql.mutation(
    function="update_post",
    context_params={
        "user_id": "input_user_id",
    },
    error_config=fraiseql.DEFAULT_ERROR_CONFIG,
)
class UpdatePost:
    """Update an existing blog post.

    Following printoptim pattern:
    - SQL function handles all validation (existence, ownership)
    - Returns mutation_result with status codes:
      - 'success:updated' -> UpdatePostSuccess
      - 'noop:not_found' -> BlogError (post not found)
      - 'error:permission_denied' -> BlogError (not post owner)
    """

    id: UUID  # Maps to input_post_id
    input: UpdatePostInput  # Maps to input_payload (converted to JSONB)
    success: UpdatePostSuccess
    failure: BlogError

    # No resolve method - SQL function handles everything


@fraiseql.mutation(
    function="create_comment",
    context_params={
        "user_id": "input_user_id",
    },
    error_config=fraiseql.DEFAULT_ERROR_CONFIG,
)
class CreateComment:
    """Create a comment on a blog post.

    Following printoptim pattern:
    - SQL function handles all validation (post existence, user existence)
    - Returns mutation_result with status codes:
      - 'success:created' -> CreateCommentSuccess
      - 'noop:not_found' -> BlogError (post not found)
      - 'noop:user_not_found' -> BlogError (user not found)
    """

    input: CreateCommentInput  # Maps to input_payload (contains post_id, content, parent_id)
    success: CreateCommentSuccess
    failure: BlogError

    # No resolve method - SQL function handles everything


# Query resolvers
@fraiseql.query
async def posts(
    info: GraphQLResolveInfo,
    where: PostWhereInput | None = None,
    order_by: list[PostOrderByInput | None] = None,
    limit: int = 20,
    offset: int = 0,
) -> list[Post]:
    """Query posts with filtering and pagination."""
    db = info.context["db"]

    # Build WHERE clause
    where_conditions = []
    params = []

    if where:
        if where.status:
            where_conditions.append("status = %s")
            params.append(where.status)
        if where.author_id:
            where_conditions.append("fk_author = %s")
            params.append(where.author_id)
        if where.title_contains:
            where_conditions.append("title ILIKE %s")
            params.append(f"%{where.title_contains}%")

    # Build ORDER BY clause
    order_clause = "created_at DESC"
    if order_by:
        order_parts = []
        for order in order_by:
            order_parts.append(f"{order.field} {order.direction}")
        order_clause = ", ".join(order_parts)

    # Build query
    where_clause = " AND ".join(where_conditions) if where_conditions else "1=1"
    query = f"""
        SELECT * FROM tb_post
        WHERE {where_clause}
        ORDER BY {order_clause}
        LIMIT %s OFFSET %s
    """
    params.extend([limit, offset])

    from fraiseql.db import DatabaseQuery

    db_query = DatabaseQuery(query, params)
    result = await db.run(db_query)
    return [Post(**row) for row in result]


@fraiseql.query
async def post(
    info: GraphQLResolveInfo, id: UUID | None = None, slug: str | None = None
) -> Post | None:
    """Get a single post by ID or slug."""
    db = info.context["db"]

    if id:
        result = await db.find_one("posts", id=id)
    elif slug:
        result = await db.find_one("posts", slug=slug)
    else:
        return None

    return result  # Repository already returns instantiated Post object or None


@fraiseql.query
async def tags(info: GraphQLResolveInfo, limit: int = 50) -> list[Tag]:
    """Get all tags."""
    db = info.context["db"]

    from fraiseql.db import DatabaseQuery
    from psycopg.sql import SQL

    query = DatabaseQuery(SQL("SELECT * FROM tb_tag ORDER BY name ASC LIMIT %s"), [limit])
    result = await db.run(query)
    return [Tag(**row) for row in result]


@fraiseql.query
async def users(info: GraphQLResolveInfo, limit: int = 20) -> list[User]:
    """Get users (admin only)."""
    db = info.context["db"]
    result = await db.find("users", limit=limit, order_by="created_at DESC")
    return result  # Repository already returns instantiated User objects


# Export collections for app registration
BLOG_TYPES = [User, Post, Comment, Tag, UserRole, PostStatus, CommentStatus]
BLOG_MUTATIONS = [CreatePost, UpdatePost, CreateComment]
BLOG_QUERIES = [posts, post, tags, users]
