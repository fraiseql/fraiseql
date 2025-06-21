"""Integration tests for GraphQL mutations."""

from unittest.mock import Mock
from uuid import uuid4

import pytest
from models import (
    CreateCommentInput,
    CreatePostInput,
    CreatePostSuccess,
    CreateUserError,
    CreateUserInput,
    CreateUserSuccess,
    UpdatePostError,
    UpdatePostInput,
    UpdatePostSuccess,
)
from mutations import create_comment, create_post, create_user, delete_post, update_post

from fraiseql.auth import UserContext


@pytest.mark.asyncio
class TestMutations:
    """Test GraphQL mutation functions."""

    def _create_info(self, blog_repo, user_context=None):
        """Create a mock info object for GraphQL context."""
        info = Mock()
        info.context = {"db": blog_repo}
        if user_context:
            info.context["user"] = user_context
        return info

    async def test_create_user_success(self, blog_repo, clean_db):
        """Test successful user creation."""
        info = self._create_info(blog_repo)
        input_data = CreateUserInput(
            email="newuser@example.com",
            name="New User",
            password="secure123",
            bio="I'm new here",
        )

        result = await create_user(info, input_data)

        assert isinstance(result, CreateUserSuccess)
        assert result.user.email == "newuser@example.com"
        assert result.user.name == "New User"
        assert result.user.bio == "I'm new here"
        assert result.message == "User created successfully"

    async def test_create_user_duplicate_email(self, blog_repo, test_user, clean_db):
        """Test user creation with duplicate email."""
        info = self._create_info(blog_repo)
        input_data = CreateUserInput(
            email=test_user.email, name="Duplicate User", password="password123",
        )

        result = await create_user(info, input_data)

        assert isinstance(result, CreateUserError)
        assert result.code == "EMAIL_EXISTS"
        assert "already registered" in result.message
        assert "email" in result.field_errors

    async def test_create_post_success(self, blog_repo, auth_context, clean_db):
        """Test successful post creation."""
        info = self._create_info(blog_repo, auth_context)
        input_data = CreatePostInput(
            title="My First Post",
            content="This is the content of my first post.",
            excerpt="First post excerpt",
            tags=["test", "first"],
            is_published=True,
        )

        result = await create_post(info, input_data)

        assert isinstance(result, CreatePostSuccess)
        assert result.post.title == "My First Post"
        assert result.post.content == "This is the content of my first post."
        assert result.post.tags == ["test", "first"]
        assert result.post.is_published is True
        assert result.post.slug == "my-first-post"

    async def test_create_post_unauthenticated(self, blog_repo, clean_db):
        """Test post creation without authentication."""
        info = self._create_info(blog_repo)  # No auth context
        input_data = CreatePostInput(
            title="Unauthorized Post", content="Should not be created",
        )

        with pytest.raises((ValueError, PermissionError)):  # Should raise auth error
            await create_post(info, input_data)

    async def test_update_post_success(
        self, blog_repo, auth_context, create_test_post, clean_db,
    ):
        """Test successful post update."""
        # Create a post first
        post = await create_test_post(
            title="Original Title", content="Original content",
        )

        info = self._create_info(blog_repo, auth_context)
        input_data = UpdatePostInput(
            title="Updated Title", content="Updated content", tags=["updated", "test"],
        )

        result = await update_post(info, post.id, input_data)

        assert isinstance(result, UpdatePostSuccess)
        assert result.post.title == "Updated Title"
        assert result.post.content == "Updated content"
        assert result.post.tags == ["updated", "test"]
        assert set(result.updated_fields) == {"title", "content", "tags"}

    async def test_update_post_not_found(self, blog_repo, auth_context, clean_db):
        """Test updating non-existent post."""
        info = self._create_info(blog_repo, auth_context)
        input_data = UpdatePostInput(title="New Title")

        result = await update_post(info, uuid4(), input_data)

        assert isinstance(result, UpdatePostError)
        assert result.code == "NOT_FOUND"

    async def test_update_post_unauthorized(self, blog_repo, test_user, clean_db):
        """Test updating another user's post."""
        # Create post as test_user
        post_result = await blog_repo.create_post(
            {
                "author_id": str(test_user.id),
                "title": "Someone else's post",
                "content": "Content",
            },
        )
        post_id = post_result["post_id"]

        # Try to update as different user
        other_user_context = UserContext(
            user_id=str(uuid4()), email="other@example.com", roles=["user"],
        )

        info = self._create_info(blog_repo, other_user_context)
        input_data = UpdatePostInput(title="Hacked Title")

        result = await update_post(info, post_id, input_data)

        assert isinstance(result, UpdatePostError)
        assert result.code == "FORBIDDEN"

    async def test_update_post_as_admin(
        self, blog_repo, admin_context, create_test_post, clean_db,
    ):
        """Test admin can update any post."""
        # Create post as regular user
        post = await create_test_post(title="User's Post")

        # Update as admin
        info = self._create_info(blog_repo, admin_context)
        input_data = UpdatePostInput(title="Admin Updated")

        result = await update_post(info, post.id, input_data)

        assert isinstance(result, UpdatePostSuccess)
        assert result.post.title == "Admin Updated"

    async def test_create_comment_success(
        self, blog_repo, auth_context, create_test_post, clean_db,
    ):
        """Test successful comment creation."""
        post = await create_test_post()

        info = self._create_info(blog_repo, auth_context)
        input_data = CreateCommentInput(
            post_id=post.id, content="Great post! Thanks for sharing.",
        )

        result = await create_comment(info, input_data)

        assert result.content == "Great post! Thanks for sharing."
        assert result.post_id == str(post.id)
        assert result.author_id == auth_context.user_id

    async def test_create_comment_reply(
        self, blog_repo, auth_context, create_test_post, create_test_comment, clean_db,
    ):
        """Test creating a reply to a comment."""
        post = await create_test_post()
        parent_comment_id = await create_test_comment(str(post.id), "Parent comment")

        info = self._create_info(blog_repo, auth_context)
        input_data = CreateCommentInput(
            post_id=post.id,
            content="Reply to your comment",
            parent_comment_id=parent_comment_id,
        )

        result = await create_comment(info, input_data)

        assert result.content == "Reply to your comment"
        assert result.parent_comment_id == parent_comment_id

    async def test_create_comment_post_not_found(
        self, blog_repo, auth_context, clean_db,
    ):
        """Test creating comment on non-existent post."""
        info = self._create_info(blog_repo, auth_context)
        input_data = CreateCommentInput(post_id=uuid4(), content="Comment on nothing")

        with pytest.raises(ValueError, match="Post not found"):
            await create_comment(info, input_data)

    async def test_delete_post_as_admin(
        self, blog_repo, admin_context, create_test_post, clean_db,
    ):
        """Test admin can delete posts."""
        post = await create_test_post()

        info = self._create_info(blog_repo, admin_context)
        result = await delete_post(info, post.id)

        assert result is True

        # Verify deletion
        deleted_post = await blog_repo.get_post_by_id(post.id)
        assert deleted_post is None

    async def test_delete_post_as_regular_user(
        self, blog_repo, auth_context, create_test_post, clean_db,
    ):
        """Test regular user cannot delete posts."""
        post = await create_test_post()

        info = self._create_info(blog_repo, auth_context)

        with pytest.raises(
            (ValueError, PermissionError),
        ):  # Should raise permission error
            await delete_post(info, post.id)

    async def test_delete_post_not_found(self, blog_repo, admin_context, clean_db):
        """Test deleting non-existent post."""
        info = self._create_info(blog_repo, admin_context)
        result = await delete_post(info, uuid4())

        assert result is False
