"""Integration tests for GraphQL queries."""

from unittest.mock import Mock
from uuid import uuid4

import pytest
from models import PostFilters, PostOrderBy
from queries import (
    get_comments_for_post,
    get_post,
    get_posts,
    get_user,
    me,
    resolve_comment_author,
    resolve_comment_replies,
    resolve_post_author,
    resolve_post_comments,
    resolve_user_posts,
)

from fraiseql.auth import UserContext


@pytest.mark.asyncio
class TestQueries:
    """Test GraphQL query functions."""

    def _create_info(self, blog_repo, user_context=None):
        """Create a mock info object for GraphQL context."""
        info = Mock()
        info.context = {"db": blog_repo}
        if user_context:
            info.context["user"] = user_context
        return info

    async def test_get_user(self, blog_repo, test_user, clean_db):
        """Test getting a user by ID."""
        info = self._create_info(blog_repo)

        result = await get_user(info, test_user.id)

        assert result is not None
        assert result.id == test_user.id
        assert result.email == test_user.email
        assert result.name == test_user.name

    async def test_get_user_not_found(self, blog_repo, clean_db):
        """Test getting non-existent user."""
        info = self._create_info(blog_repo)

        result = await get_user(info, uuid4())

        assert result is None

    async def test_me_authenticated(self, blog_repo, test_user, clean_db):
        """Test getting current authenticated user."""
        user_context = UserContext(
            user_id=str(test_user.id), email=test_user.email, roles=test_user.roles,
        )
        info = self._create_info(blog_repo, user_context)

        result = await me(info)

        assert result is not None
        assert result.id == test_user.id
        assert result.email == test_user.email

    async def test_me_unauthenticated(self, blog_repo, clean_db):
        """Test me query without authentication."""
        info = self._create_info(blog_repo)

        with pytest.raises((ValueError, PermissionError)):  # Should raise auth error
            await me(info)

    async def test_get_post(self, blog_repo, create_test_post, clean_db):
        """Test getting a post by ID."""
        post = await create_test_post(title="Test Post", view_count=5)
        info = self._create_info(blog_repo)

        # Set initial view count
        await blog_repo.connection.execute(
            "UPDATE tb_posts SET view_count = 5 WHERE id = %s", (post.id,),
        )

        result = await get_post(info, post.id)

        assert result is not None
        assert result.id == post.id
        assert result.title == "Test Post"

        # Verify view count was incremented
        updated_post = await blog_repo.get_post_by_id(post.id)
        assert updated_post.view_count == 6

    async def test_get_post_not_found(self, blog_repo, clean_db):
        """Test getting non-existent post."""
        info = self._create_info(blog_repo)

        result = await get_post(info, uuid4())

        assert result is None

    async def test_get_posts_no_filters(self, blog_repo, create_test_post, clean_db):
        """Test getting all posts without filters."""
        # Create test posts
        post1 = await create_test_post(title="Post 1")
        post2 = await create_test_post(title="Post 2")
        post3 = await create_test_post(title="Post 3")

        info = self._create_info(blog_repo)
        result = await get_posts(info)

        assert len(result) == 3
        post_ids = {p.id for p in result}
        assert post_ids == {post1.id, post2.id, post3.id}

    async def test_get_posts_with_filters(
        self, blog_repo, test_user, create_test_post, clean_db,
    ):
        """Test getting posts with filters."""
        # Create posts with different properties
        published = await create_test_post(
            title="Published", is_published=True, tags=["python"],
        )
        await create_test_post(title="Draft", is_published=False, tags=["javascript"])

        info = self._create_info(blog_repo)

        # Filter by published status
        filters = PostFilters(is_published=True)
        result = await get_posts(info, filters=filters)
        assert len(result) == 1
        assert result[0].id == published.id

        # Filter by tags
        filters = PostFilters(tags_contain=["python"])
        result = await get_posts(info, filters=filters)
        assert len(result) == 1
        assert result[0].id == published.id

        # Filter by author
        filters = PostFilters(author_id=test_user.id)
        result = await get_posts(info, filters=filters)
        assert len(result) == 2

    async def test_get_posts_ordering(self, blog_repo, create_test_post, clean_db):
        """Test getting posts with different orderings."""
        # Create posts with time gap
        import asyncio

        post1 = await create_test_post(title="First")
        await asyncio.sleep(0.1)
        post2 = await create_test_post(title="Second")

        info = self._create_info(blog_repo)

        # Test descending order
        result = await get_posts(info, order_by=PostOrderBy.CREATED_AT_DESC)
        assert result[0].id == post2.id
        assert result[1].id == post1.id

        # Test ascending order
        result = await get_posts(info, order_by=PostOrderBy.CREATED_AT_ASC)
        assert result[0].id == post1.id
        assert result[1].id == post2.id

    async def test_get_posts_pagination(self, blog_repo, create_test_post, clean_db):
        """Test getting posts with pagination."""
        # Create multiple posts
        posts = []
        for i in range(5):
            post = await create_test_post(title=f"Post {i}")
            posts.append(post)

        info = self._create_info(blog_repo)

        # Test limit
        result = await get_posts(info, limit=3)
        assert len(result) == 3

        # Test offset
        result = await get_posts(info, limit=2, offset=3)
        assert len(result) == 2

    async def test_get_comments_for_post(
        self, blog_repo, create_test_post, create_test_comment, clean_db,
    ):
        """Test getting comments for a post."""
        post = await create_test_post()

        # Create comments
        comment1_id = await create_test_comment(str(post.id), "First comment")
        await create_test_comment(str(post.id), "Second comment")
        await create_test_comment(str(post.id), "Reply", parent_id=comment1_id)

        info = self._create_info(blog_repo)
        result = await get_comments_for_post(info, post.id)

        assert len(result) == 3
        contents = {c.content for c in result}
        assert contents == {"First comment", "Second comment", "Reply"}

    async def test_resolve_post_author(
        self, blog_repo, test_user, create_test_post, clean_db,
    ):
        """Test resolving post author."""
        post = await create_test_post()

        info = self._create_info(blog_repo)
        author = await resolve_post_author(post, info)

        assert author is not None
        assert author.id == test_user.id
        assert author.email == test_user.email

    async def test_resolve_post_comments(
        self, blog_repo, create_test_post, create_test_comment, clean_db,
    ):
        """Test resolving post comments."""
        post = await create_test_post()
        await create_test_comment(str(post.id), "Comment 1")
        await create_test_comment(str(post.id), "Comment 2")

        info = self._create_info(blog_repo)
        comments = await resolve_post_comments(post, info)

        assert len(comments) == 2
        contents = {c.content for c in comments}
        assert contents == {"Comment 1", "Comment 2"}

    async def test_resolve_comment_author(
        self, blog_repo, test_user, create_test_post, create_test_comment, clean_db,
    ):
        """Test resolving comment author."""
        post = await create_test_post()
        await create_test_comment(str(post.id), "Test comment")

        comments = await blog_repo.get_comments_by_post(post.id)
        comment = comments[0]

        info = self._create_info(blog_repo)
        author = await resolve_comment_author(comment, info)

        assert author is not None
        assert author.id == test_user.id

    async def test_resolve_comment_replies(
        self, blog_repo, create_test_post, create_test_comment, clean_db,
    ):
        """Test resolving comment replies."""
        post = await create_test_post()
        parent_id = await create_test_comment(str(post.id), "Parent comment")
        await create_test_comment(str(post.id), "Reply 1", parent_id=parent_id)
        await create_test_comment(str(post.id), "Reply 2", parent_id=parent_id)

        comments = await blog_repo.get_comments_by_post(post.id)
        parent_comment = next(c for c in comments if c.content == "Parent comment")

        info = self._create_info(blog_repo)
        replies = await resolve_comment_replies(parent_comment, info)

        assert len(replies) == 2
        reply_contents = {r.content for r in replies}
        assert reply_contents == {"Reply 1", "Reply 2"}

    async def test_resolve_user_posts(
        self, blog_repo, test_user, create_test_post, clean_db,
    ):
        """Test resolving user's posts."""
        # Create posts for the user
        await create_test_post(title="User Post 1")
        await create_test_post(title="User Post 2")

        # Create post for different user
        other_user_result = await blog_repo.create_user(
            {"email": "other@example.com", "name": "Other User"},
        )
        await blog_repo.create_post(
            {
                "author_id": other_user_result["user_id"],
                "title": "Other User Post",
                "content": "Content",
            },
        )

        info = self._create_info(blog_repo)
        user_posts = await resolve_user_posts(test_user, info)

        assert len(user_posts) == 2
        post_titles = {p.title for p in user_posts}
        assert post_titles == {"User Post 1", "User Post 2"}
