"""Unit tests for BlogRepository."""

import pytest
from db import BlogRepository


@pytest.mark.asyncio
class TestBlogRepository:
    """Test BlogRepository methods."""

    async def test_create_user(self, blog_repo: BlogRepository, clean_db):
        """Test creating a user."""
        # Create user
        result = await blog_repo.create_user(
            {
                "email": "newuser@example.com",
                "name": "New User",
                "bio": "A new user",
                "avatar_url": "https://example.com/avatar.jpg",
            },
        )

        assert result["success"] is True
        assert "user_id" in result

        # Verify user was created
        user = await blog_repo.get_user_by_id(result["user_id"])
        assert user is not None
        assert user["email"] == "newuser@example.com"
        assert user["name"] == "New User"
        assert user["bio"] == "A new user"
        assert user["avatarUrl"] == "https://example.com/avatar.jpg"
        assert user["isActive"] is True
        assert user["roles"] == ["user"]

    async def test_create_user_duplicate_email(
        self, blog_repo: BlogRepository, test_user, clean_db,
    ):
        """Test creating a user with duplicate email fails."""
        result = await blog_repo.create_user(
            {"email": test_user["email"], "name": "Duplicate User"},
        )

        assert result["success"] is False
        assert "already exists" in result["error"]

    async def test_get_user_by_email(
        self, blog_repo: BlogRepository, test_user, clean_db,
    ):
        """Test getting a user by email."""
        user = await blog_repo.get_user_by_email(test_user["email"])

        assert user is not None
        assert user["id"] == test_user["id"]
        assert user["email"] == test_user["email"]

    async def test_get_user_by_email_not_found(
        self, blog_repo: BlogRepository, clean_db,
    ):
        """Test getting a non-existent user by email."""
        user = await blog_repo.get_user_by_email("nonexistent@example.com")
        assert user is None

    async def test_create_post(self, blog_repo: BlogRepository, test_user, clean_db):
        """Test creating a post."""
        result = await blog_repo.create_post(
            {
                "author_id": str(test_user["id"]),
                "title": "Test Post",
                "content": "This is test content",
                "excerpt": "Test excerpt",
                "tags": ["test", "example"],
                "is_published": True,
            },
        )

        assert result["success"] is True
        assert "post_id" in result
        assert "slug" in result

        # Verify post was created
        post = await blog_repo.get_post_by_id(result["post_id"])
        assert post is not None
        assert post["title"] == "Test Post"
        assert post["content"] == "This is test content"
        assert post["excerpt"] == "Test excerpt"
        assert post["tags"] == ["test", "example"]
        assert post["isPublished"] is True
        assert post["publishedAt"] is not None
        assert post["viewCount"] == 0

    async def test_create_post_generates_unique_slug(
        self, blog_repo: BlogRepository, test_user, clean_db,
    ):
        """Test that posts with same title get unique slugs."""
        # Create first post
        result1 = await blog_repo.create_post(
            {
                "author_id": str(test_user["id"]),
                "title": "Duplicate Title",
                "content": "Content 1",
            },
        )

        # Create second post with same title
        result2 = await blog_repo.create_post(
            {
                "author_id": str(test_user["id"]),
                "title": "Duplicate Title",
                "content": "Content 2",
            },
        )

        assert result1["success"] is True
        assert result2["success"] is True
        assert result1["slug"] != result2["slug"]

    async def test_update_post(
        self, blog_repo: BlogRepository, test_user, create_test_post, clean_db,
    ):
        """Test updating a post."""
        post = await create_test_post(title="Original Title", is_published=False)

        result = await blog_repo.update_post(
            {
                "id": str(post.id),
                "title": "Updated Title",
                "content": "Updated content",
                "is_published": True,
            },
        )

        assert result["success"] is True

        # Verify update
        updated_post = await blog_repo.get_post_by_id(post.id)
        assert updated_post.title == "Updated Title"
        assert updated_post.content == "Updated content"
        assert updated_post.is_published is True
        assert updated_post.published_at is not None

    async def test_delete_post(
        self, blog_repo: BlogRepository, create_test_post, clean_db,
    ):
        """Test deleting a post."""
        post = await create_test_post()

        result = await blog_repo.delete_post(post.id)
        assert result["success"] is True

        # Verify deletion
        deleted_post = await blog_repo.get_post_by_id(post.id)
        assert deleted_post is None

    async def test_get_posts_with_filters(
        self, blog_repo: BlogRepository, test_user, create_test_post, clean_db,
    ):
        """Test getting posts with various filters."""
        # Create test posts
        published_post = await create_test_post(
            title="Published", is_published=True, tags=["python", "tutorial"],
        )
        await create_test_post(title="Draft", is_published=False, tags=["javascript"])

        # Test filter by published status
        published_posts = await blog_repo.get_posts(filters={"is_published": True})
        assert len(published_posts) == 1
        assert published_posts[0].id == published_post.id

        # Test filter by author
        author_posts = await blog_repo.get_posts(
            filters={"author_id": str(test_user.id)},
        )
        assert len(author_posts) == 2

        # Test filter by tags
        python_posts = await blog_repo.get_posts(filters={"tags": ["python"]})
        assert len(python_posts) == 1
        assert python_posts[0].id == published_post.id

    async def test_get_posts_with_ordering(
        self, blog_repo: BlogRepository, create_test_post, clean_db,
    ):
        """Test getting posts with ordering."""
        # Create posts with different timestamps
        post1 = await create_test_post(title="First Post")
        post2 = await create_test_post(title="Second Post")

        # Test descending order (default)
        posts_desc = await blog_repo.get_posts(order_by="created_at_desc")
        assert posts_desc[0].id == post2.id
        assert posts_desc[1].id == post1.id

        # Test ascending order
        posts_asc = await blog_repo.get_posts(order_by="created_at_asc")
        assert posts_asc[0].id == post1.id
        assert posts_asc[1].id == post2.id

    async def test_get_posts_pagination(
        self, blog_repo: BlogRepository, create_test_post, clean_db,
    ):
        """Test getting posts with pagination."""
        # Create multiple posts
        posts = []
        for i in range(5):
            post = await create_test_post(title=f"Post {i}")
            posts.append(post)

        # Test limit
        limited_posts = await blog_repo.get_posts(limit=2)
        assert len(limited_posts) == 2

        # Test offset
        offset_posts = await blog_repo.get_posts(limit=2, offset=2)
        assert len(offset_posts) == 2
        assert offset_posts[0].id == posts[2].id

    async def test_create_comment(
        self, blog_repo: BlogRepository, test_user, create_test_post, clean_db,
    ):
        """Test creating a comment."""
        post = await create_test_post()

        result = await blog_repo.create_comment(
            {
                "post_id": str(post.id),
                "author_id": str(test_user["id"]),
                "content": "Great post!",
            },
        )

        assert result["success"] is True
        assert "comment_id" in result

        # Verify comment was created
        comments = await blog_repo.get_comments_by_post(post.id)
        assert len(comments) == 1
        assert comments[0].content == "Great post!"
        assert comments[0].post_id == str(post.id)
        assert comments[0]["authorId"] == str(test_user["id"])

    async def test_create_nested_comment(
        self,
        blog_repo: BlogRepository,
        test_user,
        create_test_post,
        create_test_comment,
        clean_db,
    ):
        """Test creating a reply to a comment."""
        post = await create_test_post()
        parent_id = await create_test_comment(str(post.id), "Parent comment")

        result = await blog_repo.create_comment(
            {
                "post_id": str(post.id),
                "author_id": str(test_user["id"]),
                "content": "Reply to parent",
                "parent_id": parent_id,
            },
        )

        assert result["success"] is True

        # Verify nested structure
        comments = await blog_repo.get_comments_by_post(post.id)
        assert len(comments) == 2

        reply = next(c for c in comments if c.content == "Reply to parent")
        assert reply.parent_comment_id == parent_id

    async def test_increment_view_count(
        self, blog_repo: BlogRepository, create_test_post, clean_db,
    ):
        """Test incrementing post view count."""
        post = await create_test_post()
        assert post.view_count == 0

        # Increment view count
        result = await blog_repo.increment_view_count(post.id)
        assert result["success"] is True

        # Verify increment
        updated_post = await blog_repo.get_post_by_id(post.id)
        assert updated_post.view_count == 1

        # Increment again
        await blog_repo.increment_view_count(post.id)
        updated_post = await blog_repo.get_post_by_id(post.id)
        assert updated_post.view_count == 2
