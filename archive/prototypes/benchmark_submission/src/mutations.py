"""GraphQL Benchmark Mutations - FraiseQL Implementation

This module implements all GraphQL mutations required by the benchmark schema,
following CQRS architecture with write operations to tb_ tables and automatic
sync to tv_ tables.
"""

from uuid import UUID

import fraiseql
from fraiseql import Info

from .db import BenchmarkRepository
from .models import (
    CreatePostInput,
    CreatePostResult,
    CreateUserInput,
    CreateUserResult,
    DeleteUserResult,
    UpdateUserInput,
    UpdateUserResult,
)


@fraiseql.mutation
async def create_user(info: Info, input: CreateUserInput) -> CreateUserResult:
    """Create a new user."""
    db: BenchmarkRepository = info.context["db"]

    try:
        user_data = await db.create_user(input)
        user = await db.get_user_with_posts(user_data["id"])

        # Convert to User object with posts
        from .models import Post, User

        user_obj = User(**user_data)
        user_obj.posts = [Post(**post_data) for post_data in user.get("posts", [])]

        return CreateUserResult(success=True, user=user_obj, message="User created successfully")
    except Exception as e:
        return CreateUserResult(success=False, user=None, message=f"Failed to create user: {e!s}")


@fraiseql.mutation
async def update_user(info: Info, id: UUID, input: UpdateUserInput) -> UpdateUserResult:
    """Update an existing user."""
    db: BenchmarkRepository = info.context["db"]

    try:
        # Check if user exists
        existing_user = await db.get_user_by_id(id)
        if not existing_user:
            return UpdateUserResult(success=False, user=None, message="User not found")

        # Update user
        updated_data = await db.update_user(id, input)
        user = await db.get_user_with_posts(id)

        # Convert to User object with posts
        from .models import Post, User

        user_obj = User(**updated_data)
        user_obj.posts = [Post(**post_data) for post_data in user.get("posts", [])]

        return UpdateUserResult(success=True, user=user_obj, message="User updated successfully")
    except Exception as e:
        return UpdateUserResult(success=False, user=None, message=f"Failed to update user: {e!s}")


@fraiseql.mutation
async def delete_user(info: Info, id: UUID) -> DeleteUserResult:
    """Delete a user."""
    db: BenchmarkRepository = info.context["db"]

    try:
        # Check if user exists
        existing_user = await db.get_user_by_id(id)
        if not existing_user:
            return DeleteUserResult(success=False, message="User not found")

        # Delete user (this will cascade to posts and comments)
        await db.delete_user(id)

        return DeleteUserResult(success=True, message="User deleted successfully")
    except Exception as e:
        return DeleteUserResult(success=False, message=f"Failed to delete user: {e!s}")


@fraiseql.mutation
async def create_post(info: Info, input: CreatePostInput) -> CreatePostResult:
    """Create a new post."""
    db: BenchmarkRepository = info.context["db"]

    try:
        # Verify author exists
        author = await db.get_user_by_id(input.author_id)
        if not author:
            return CreatePostResult(success=False, post=None, message="Author not found")

        post_data = await db.create_post(input)
        post = await db.get_post_with_author_and_comments(post_data["id"])

        # Convert to Post object with nested data
        from .models import Comment, Post, User

        post_obj = Post(**post_data)
        post_obj.author = User(**post.get("author", {}))
        post_obj.comments = [Comment(**comment_data) for comment_data in post.get("comments", [])]

        return CreatePostResult(success=True, post=post_obj, message="Post created successfully")
    except Exception as e:
        return CreatePostResult(success=False, post=None, message=f"Failed to create post: {e!s}")
