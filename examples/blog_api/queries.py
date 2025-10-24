"""Example blog API queries using FraiseQL with CQRS."""

from typing import TYPE_CHECKING
from uuid import UUID

from models import Comment, Post, PostFilters, PostOrderBy, User

import fraiseql
from fraiseql.auth import requires_auth

if TYPE_CHECKING:
    from db import BlogRepository


@fraiseql.query
async def user(info, id: UUID) -> User | None:
    """Get a user by ID."""
    db: BlogRepository = info.context["db"]
    user_data = await db.get_user_by_id(id)
    return User.from_dict(user_data) if user_data else None


@fraiseql.query
@requires_auth
async def me(info) -> User | None:
    """Get the current authenticated user."""
    db: BlogRepository = info.context["db"]
    user_context = info.context["user"]
    user_data = await db.get_user_by_id(UUID(user_context.user_id))
    return User.from_dict(user_data) if user_data else None


@fraiseql.query
async def post(info, id: UUID) -> Post | None:
    """Get a post by ID."""
    db: BlogRepository = info.context["db"]

    post_data = await db.get_post_by_id(id)

    if not post_data:
        return None

    # Increment view count asynchronously
    await db.increment_view_count(id)

    return Post.from_dict(post_data)


@fraiseql.query
async def posts(
    info,
    filters: PostFilters | None = None,
    order_by: PostOrderBy | None = None,
    limit: int = 20,
    offset: int = 0,
) -> list[Post]:
    """Get posts with optional filtering and pagination."""
    db: BlogRepository = info.context["db"]

    # Convert filters to dict
    filter_dict = {}
    if filters:
        if filters.is_published is not None:
            filter_dict["is_published"] = filters.is_published
        if filters.author_id:
            filter_dict["author_id"] = filters.author_id
        if filters.tags_contain:
            filter_dict["tags"] = filters.tags_contain

    # Set up ordering
    order_clause = "created_at DESC"
    if order_by:
        order_clause = f"{order_by.field} {order_by.direction.upper()}"

    # Get posts from view
    posts_data = await db.get_posts(
        filters=filter_dict,
        order_by=order_clause,
        limit=limit,
        offset=offset,
    )

    return [Post.from_dict(data) for data in posts_data]


@fraiseql.query
async def comments(info, post_id: UUID) -> list[Comment]:
    """Get all comments for a post."""
    db: BlogRepository = info.context["db"]
    comments_data = await db.get_comments_by_post(post_id)
    return [Comment.from_dict(data) for data in comments_data]


# Field resolvers for related data


async def resolve_post_author(post: Post, info) -> User | None:
    """Resolve the author field for a post."""
    if not post.author_id:
        return None

    db: BlogRepository = info.context["db"]
    user_data = await db.get_user_by_id(UUID(post.author_id))
    return User.from_dict(user_data) if user_data else None


async def resolve_comment_author(comment: Comment, info) -> User | None:
    """Resolve the author field for a comment."""
    if not comment.author_id:
        return None

    db: BlogRepository = info.context["db"]
    user_data = await db.get_user_by_id(UUID(comment.author_id))
    return User.from_dict(user_data) if user_data else None


async def resolve_comment_post(comment: Comment, info) -> Post | None:
    """Resolve the post field for a comment."""
    if not comment.post_id:
        return None

    db: BlogRepository = info.context["db"]
    post_data = await db.get_post_by_id(UUID(comment.post_id))
    return Post.from_dict(post_data) if post_data else None


async def resolve_comment_replies(comment: Comment, info) -> list[Comment]:
    """Resolve replies for a comment."""
    db: BlogRepository = info.context["db"]
    all_comments = await db.get_comments_by_post(UUID(comment.post_id))

    # Filter for replies to this comment
    if not all_comments:
        return []
    return [Comment.from_dict(c) for c in all_comments if c.get("parentCommentId") == comment.id]


async def resolve_user_posts(user: User, info) -> list[Post]:
    """Resolve posts for a user."""
    db: BlogRepository = info.context["db"]

    posts_data = await db.get_posts(filters={"author_id": user.id})
    return [Post.from_dict(data) for data in posts_data]
