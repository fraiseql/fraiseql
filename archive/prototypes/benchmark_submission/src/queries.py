"""GraphQL Benchmark Queries - FraiseQL Implementation

This module implements all GraphQL queries required by the benchmark schema,
following CQRS architecture with denormalized read models and DataLoader
optimization for N+1 query prevention.
"""

from typing import Optional
from uuid import UUID

import fraiseql
from fraiseql import Info

from .db import BenchmarkRepository
from .models import Comment, OrderBy, Post, User, UserFilter

# ==============================================================================
# SIMPLE QUERIES
# ==============================================================================


@fraiseql.query
async def users(info: Info, limit: int = 20, offset: int = 0) -> list[User]:
    """Get users with optional pagination.

    Returns denormalized user data with their posts included.
    """
    db: BenchmarkRepository = info.context["db"]

    # Get users with their posts denormalized
    users_data = await db.get_users_with_posts(limit=limit, offset=offset)

    # Convert to User objects with posts included
    users = []
    for user_data in users_data:
        user = User(**user_data["user"])
        user.posts = [Post(**post_data) for post_data in user_data["posts"]]
        users.append(user)

    return users


@fraiseql.query
async def user(info: Info, id: UUID) -> Optional[User]:
    """Get a single user by ID with their posts."""
    db: BenchmarkRepository = info.context["db"]

    user_data = await db.get_user_with_posts(id)
    if not user_data:
        return None

    user = User(**user_data["user"])
    user.posts = [Post(**post_data) for post_data in user_data["posts"]]
    return user


# ==============================================================================
# COMPLEX FILTERING QUERIES
# ==============================================================================


@fraiseql.query
async def users_where(
    info: Info,
    where: Optional[UserFilter] = None,
    order_by: Optional[OrderBy] = None,
    limit: int = 20,
) -> list[User]:
    """Get users with complex filtering and ordering."""
    db: BenchmarkRepository = info.context["db"]

    # Build filter conditions
    filters = {}
    if where:
        if where.age_gt is not None:
            filters["age_gt"] = where.age_gt
        if where.age_lt is not None:
            filters["age_lt"] = where.age_lt
        if where.city:
            filters["city"] = where.city
        if where.name_contains:
            filters["name_contains"] = where.name_contains

    # Build order clause
    order_clause = None
    if order_by:
        order_clause = f"{order_by.field} {order_by.direction.value}"

    users_data = await db.get_users_filtered(filters=filters, order_by=order_clause, limit=limit)

    # Convert to User objects (without posts for performance)
    return [User(**user_data) for user_data in users_data]


# ==============================================================================
# N+1 PREVENTION QUERIES
# ==============================================================================


@fraiseql.query
async def users_with_posts(info: Info, limit: int = 50) -> list[User]:
    """Get users with their posts - tests N+1 query prevention."""
    # This is the same as the users query above - DataLoader handles N+1 prevention
    return await users(info, limit=limit, offset=0)


# ==============================================================================
# POST QUERIES
# ==============================================================================


@fraiseql.query
async def posts(info: Info, limit: int = 20, offset: int = 0) -> list[Post]:
    """Get posts with optional pagination.

    Returns denormalized post data with author and comments included.
    """
    db: BenchmarkRepository = info.context["db"]

    # Get posts with authors and comments denormalized
    posts_data = await db.get_posts_with_authors_and_comments(limit=limit, offset=offset)

    # Convert to Post objects with nested data
    posts = []
    for post_data in posts_data:
        post = Post(**post_data["post"])
        post.author = User(**post_data["author"])
        post.comments = [Comment(**comment_data) for comment_data in post_data["comments"]]
        posts.append(post)

    return posts


@fraiseql.query
async def post(info: Info, id: UUID) -> Optional[Post]:
    """Get a single post by ID with author and comments."""
    db: BenchmarkRepository = info.context["db"]

    post_data = await db.get_post_with_author_and_comments(id)
    if not post_data:
        return None

    post = Post(**post_data["post"])
    post.author = User(**post_data["author"])
    post.comments = [Comment(**comment_data) for comment_data in post_data["comments"]]
    return post


# ==============================================================================
# FIELD RESOLVERS (for nested data access)
# ==============================================================================


async def resolve_user_posts(user: User, info: Info) -> list[Post]:
    """Resolve posts field for User type."""
    db: BenchmarkRepository = info.context["db"]
    posts_data = await db.get_posts_by_author(user.id)
    return [Post(**post_data) for post_data in posts_data]


async def resolve_post_author(post: Post, info: Info) -> Optional[User]:
    """Resolve author field for Post type."""
    # Author is already denormalized in our queries, but this resolver
    # handles cases where we need to fetch it separately
    user_loader = info.context["user_loader"]
    user_data = await user_loader.load(post.author_id)
    return User(**user_data) if user_data else None


async def resolve_post_comments(post: Post, info: Info) -> list[Comment]:
    """Resolve comments field for Post type."""
    # Comments are already denormalized in our queries, but this resolver
    # handles cases where we need to fetch them separately
    db: BenchmarkRepository = info.context["db"]
    comments_data = await db.get_comments_by_post(post.id)
    return [Comment(**comment_data) for comment_data in comments_data]


async def resolve_comment_author(comment: Comment, info: Info) -> Optional[User]:
    """Resolve author field for Comment type."""
    user_loader = info.context["user_loader"]
    user_data = await user_loader.load(comment.author_id)
    return User(**user_data) if user_data else None


async def resolve_comment_post(comment: Comment, info: Info) -> Optional[Post]:
    """Resolve post field for Comment type."""
    post_loader = info.context["post_loader"]
    post_data = await post_loader.load(comment.post_id)
    return Post(**post_data) if post_data else None
