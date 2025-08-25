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
from typing import Any, Dict, List, Optional, Union
from uuid import UUID

from graphql import GraphQLResolveInfo

import fraiseql


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
@fraiseql.type(sql_source="users")
class User:
    """User with profile and authentication."""

    id: UUID
    username: str
    email: str
    role: UserRole
    created_at: datetime
    profile_data: Optional[Dict[str, Any]]

    @fraiseql.field
    async def posts(self, info: GraphQLResolveInfo) -> List["Post"]:
        """User's posts."""
        db = info.context["db"]
        return await db.find("posts", author_id=self.id, order_by="created_at DESC")

    @fraiseql.field
    async def full_name(self, info: GraphQLResolveInfo) -> Optional[str]:
        """Full name from profile data."""
        if self.profile_data:
            first = self.profile_data.get("first_name", "")
            last = self.profile_data.get("last_name", "")
            if first or last:
                return f"{first} {last}".strip()
        return None


@fraiseql.type(sql_source="posts")
class Post:
    """Blog post with content and metadata."""

    id: UUID
    title: str
    slug: str
    content: str
    excerpt: Optional[str]
    author_id: UUID
    status: PostStatus
    published_at: Optional[datetime]
    created_at: datetime

    @fraiseql.field
    async def author(self, info: GraphQLResolveInfo) -> User:
        """Post author."""
        db = info.context["db"]
        return await db.find_one("users", id=self.author_id)

    @fraiseql.field
    async def tags(self, info: GraphQLResolveInfo) -> List["Tag"]:
        """Post tags."""
        db = info.context["db"]
        # Join through post_tags table
        result = await db.execute(
            """
            SELECT t.* FROM tags t
            JOIN post_tags pt ON t.id = pt.tag_id
            WHERE pt.post_id = %s
        """,
            [self.id],
        )
        return [Tag(**row) for row in result]

    @fraiseql.field
    async def comments(self, info: GraphQLResolveInfo) -> List["Comment"]:
        """Post comments."""
        db = info.context["db"]
        return await db.find("comments", post_id=self.id, status=CommentStatus.APPROVED)

    @fraiseql.field
    async def comment_count(self, info: GraphQLResolveInfo) -> int:
        """Number of approved comments."""
        db = info.context["db"]
        result = await db.execute(
            "SELECT COUNT(*) as count FROM comments WHERE post_id = %s AND status = %s",
            [self.id, CommentStatus.APPROVED],
        )
        return result[0]["count"] if result else 0


@fraiseql.type(sql_source="comments")
class Comment:
    """Comment with threading support."""

    id: UUID
    post_id: UUID
    author_id: UUID
    parent_id: Optional[UUID]
    content: str
    status: CommentStatus
    created_at: datetime

    @fraiseql.field
    async def author(self, info: GraphQLResolveInfo) -> User:
        """Comment author."""
        db = info.context["db"]
        return await db.find_one("users", id=self.author_id)

    @fraiseql.field
    async def post(self, info: GraphQLResolveInfo) -> Post:
        """Comment's post."""
        db = info.context["db"]
        return await db.find_one("posts", id=self.post_id)

    @fraiseql.field
    async def parent(self, info: GraphQLResolveInfo) -> Optional["Comment"]:
        """Parent comment if reply."""
        if not self.parent_id:
            return None
        db = info.context["db"]
        return await db.find_one("comments", id=self.parent_id)

    @fraiseql.field
    async def replies(self, info: GraphQLResolveInfo) -> List["Comment"]:
        """Replies to this comment."""
        db = info.context["db"]
        return await db.find("comments", parent_id=self.id, status=CommentStatus.APPROVED)


@fraiseql.type(sql_source="tags")
class Tag:
    """Content tag/category."""

    id: UUID
    name: str
    slug: str
    color: Optional[str]
    description: Optional[str]

    @fraiseql.field
    async def posts(self, info: GraphQLResolveInfo) -> List[Post]:
        """Posts with this tag."""
        db = info.context["db"]
        result = await db.execute(
            """
            SELECT p.* FROM posts p
            JOIN post_tags pt ON p.id = pt.post_id
            WHERE pt.tag_id = %s AND p.status = %s
            ORDER BY p.created_at DESC
        """,
            [self.id, PostStatus.PUBLISHED],
        )
        return [Post(**row) for row in result]

    @fraiseql.field
    async def post_count(self, info: GraphQLResolveInfo) -> int:
        """Number of published posts with this tag."""
        db = info.context["db"]
        result = await db.execute(
            """
            SELECT COUNT(*) as count FROM posts p
            JOIN post_tags pt ON p.id = pt.post_id
            WHERE pt.tag_id = %s AND p.status = %s
        """,
            [self.id, PostStatus.PUBLISHED],
        )
        return result[0]["count"] if result else 0


# Input types
@fraiseql.input
class CreatePostInput:
    """Input for creating a blog post."""

    title: str
    content: str
    excerpt: Optional[str] = None
    tag_ids: Optional[List[UUID]] = None


@fraiseql.input
class UpdatePostInput:
    """Input for updating a blog post."""

    title: Optional[str] = None
    content: Optional[str] = None
    excerpt: Optional[str] = None
    status: Optional[PostStatus] = None
    tag_ids: Optional[List[UUID]] = None


@fraiseql.input
class CreateCommentInput:
    """Input for creating a comment."""

    post_id: UUID
    content: str
    parent_id: Optional[UUID] = None


@fraiseql.input
class CreateTagInput:
    """Input for creating a tag."""

    name: str
    color: Optional[str] = "#6366f1"
    description: Optional[str] = None


@fraiseql.input
class CreateUserInput:
    """Input for creating a user."""

    username: str
    email: str
    password: str
    role: UserRole = UserRole.USER
    profile_data: Optional[Dict[str, Any]] = None


# Filter inputs
@fraiseql.input
class PostWhereInput:
    """Filter posts by various criteria."""

    status: Optional[PostStatus] = None
    author_id: Optional[UUID] = None
    title_contains: Optional[str] = None
    tag_ids: Optional[List[UUID]] = None


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
    field_errors: Optional[List[Dict[str, str]]] = None


@fraiseql.failure
class NotFoundError:
    """Entity not found error."""

    message: str
    code: str = "NOT_FOUND"
    entity_type: Optional[str] = None
    entity_id: Optional[UUID] = None


@fraiseql.failure
class PermissionError:
    """Permission denied error."""

    message: str
    code: str = "PERMISSION_DENIED"
    required_role: Optional[str] = None


# Mutation classes
@fraiseql.mutation
class CreatePost:
    """Create a new blog post."""

    input: CreatePostInput
    success: CreatePostSuccess
    failure: Union[ValidationError, PermissionError]

    async def resolve(
        self, info: GraphQLResolveInfo
    ) -> Union[CreatePostSuccess, ValidationError, PermissionError]:
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
            return ValidationError(message=f"Failed to create post: {e!s}")


@fraiseql.mutation
class UpdatePost:
    """Update an existing blog post."""

    id: UUID
    input: UpdatePostInput
    success: UpdatePostSuccess
    failure: Union[ValidationError, NotFoundError, PermissionError]

    async def resolve(
        self, info: GraphQLResolveInfo
    ) -> Union[UpdatePostSuccess, ValidationError, NotFoundError, PermissionError]:
        db = info.context["db"]
        user_id = info.context["user_id"]

        try:
            # Check if post exists and user has permission
            existing_post = await db.find_one("posts", id=self.id)
            if not existing_post:
                return NotFoundError(
                    message="Post not found", entity_type="Post", entity_id=self.id
                )

            if existing_post["author_id"] != user_id:
                return PermissionError(message="You can only edit your own posts")

            # Build update data
            update_data = {}
            if self.input.title is not None:
                update_data["title"] = self.input.title
                update_data["slug"] = self.input.title.lower().replace(" ", "-").replace("_", "-")
            if self.input.content is not None:
                update_data["content"] = self.input.content
            if self.input.excerpt is not None:
                update_data["excerpt"] = self.input.excerpt
            if self.input.status is not None:
                update_data["status"] = self.input.status
                if self.input.status == PostStatus.PUBLISHED:
                    update_data["published_at"] = datetime.utcnow()

            # Update post
            await db.update("posts", update_data, id=self.id)

            # Update tags if provided
            if self.input.tag_ids is not None:
                # Remove existing tags
                await db.execute("DELETE FROM post_tags WHERE post_id = %s", [self.id])
                # Add new tags
                for tag_id in self.input.tag_ids:
                    await db.insert("post_tags", {"post_id": self.id, "tag_id": tag_id})

            # Return updated post
            post = await db.find_one("posts", id=self.id)
            return UpdatePostSuccess(post=Post(**post))

        except Exception as e:
            return ValidationError(message=f"Failed to update post: {e!s}")


@fraiseql.mutation
class CreateComment:
    """Create a comment on a blog post."""

    input: CreateCommentInput
    success: CreateCommentSuccess
    failure: Union[ValidationError, NotFoundError]

    async def resolve(
        self, info: GraphQLResolveInfo
    ) -> Union[CreateCommentSuccess, ValidationError, NotFoundError]:
        db = info.context["db"]
        user_id = info.context["user_id"]

        try:
            # Check if post exists
            post = await db.find_one("posts", id=self.input.post_id)
            if not post:
                return NotFoundError(
                    message="Post not found", entity_type="Post", entity_id=self.input.post_id
                )

            # Create comment
            comment_data = {
                "post_id": self.input.post_id,
                "author_id": user_id,
                "parent_id": self.input.parent_id,
                "content": self.input.content,
                "status": CommentStatus.PENDING,  # Requires moderation
            }

            comment_id = await db.insert("comments", comment_data, returning="id")
            comment = await db.find_one("comments", id=comment_id)

            return CreateCommentSuccess(comment=Comment(**comment))

        except Exception as e:
            return ValidationError(message=f"Failed to create comment: {e!s}")


# Query resolvers
@fraiseql.query
async def posts(
    info: GraphQLResolveInfo,
    where: Optional[PostWhereInput] = None,
    order_by: Optional[List[PostOrderByInput]] = None,
    limit: int = 20,
    offset: int = 0,
) -> List[Post]:
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
            where_conditions.append("author_id = %s")
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
        SELECT * FROM posts
        WHERE {where_clause}
        ORDER BY {order_clause}
        LIMIT %s OFFSET %s
    """
    params.extend([limit, offset])

    result = await db.execute(query, params)
    return [Post(**row) for row in result]


@fraiseql.query
async def post(
    info: GraphQLResolveInfo, id: Optional[UUID] = None, slug: Optional[str] = None
) -> Optional[Post]:
    """Get a single post by ID or slug."""
    db = info.context["db"]

    if id:
        result = await db.find_one("posts", id=id)
    elif slug:
        result = await db.find_one("posts", slug=slug)
    else:
        return None

    return Post(**result) if result else None


@fraiseql.query
async def tags(info: GraphQLResolveInfo, limit: int = 50) -> List[Tag]:
    """Get all tags."""
    db = info.context["db"]
    result = await db.find("tags", limit=limit, order_by="name ASC")
    return [Tag(**row) for row in result]


@fraiseql.query
async def users(info: GraphQLResolveInfo, limit: int = 20) -> List[User]:
    """Get users (admin only)."""
    db = info.context["db"]
    result = await db.find("users", limit=limit, order_by="created_at DESC")
    return [User(**row) for row in result]


# Export collections for app registration
BLOG_TYPES = [User, Post, Comment, Tag, UserRole, PostStatus, CommentStatus]
BLOG_MUTATIONS = [CreatePost, UpdatePost, CreateComment]
BLOG_QUERIES = [posts, post, tags, users]
