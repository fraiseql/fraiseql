"""Test DataLoader integration with the blog API example."""

import sys
from pathlib import Path

# Add the blog_api directory to the path so we can import its modules
blog_api_path = Path(__file__).parent / "../../examples/blog_api"
sys.path.insert(0, str(blog_api_path))

from unittest.mock import Mock
from uuid import UUID

import pytest

# Import from the blog example
from dataloaders import CommentsByPostDataLoader, PostDataLoader, UserDataLoader
from fastapi.testclient import TestClient

import fraiseql
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.gql.schema_builder import SchemaRegistry
from fraiseql.optimization.registry import get_loader


@pytest.fixture(autouse=True)
def clear_registry():
    """Clear registry before each test."""
    registry = SchemaRegistry.get_instance()
    registry.clear()
    yield
    registry.clear()


@pytest.fixture
def mock_blog_db():
    """Create a mock blog database with test data."""

    class MockBlogDB:
        def __init__(self) -> None:
            self.users = {
                "223e4567-e89b-12d3-a456-426614174001": {
                    "id": "223e4567-e89b-12d3-a456-426614174001",
                    "name": "John Doe",
                    "email": "john@example.com",
                    "bio": "A test user",
                },
                "323e4567-e89b-12d3-a456-426614174002": {
                    "id": "323e4567-e89b-12d3-a456-426614174002",
                    "name": "Jane Smith",
                    "email": "jane@example.com",
                    "bio": "Another test user",
                },
            }

            self.posts = {
                "123e4567-e89b-12d3-a456-426614174000": {
                    "id": "123e4567-e89b-12d3-a456-426614174000",
                    "title": "Test Post 1",
                    "content": "Content 1",
                    "authorId": "223e4567-e89b-12d3-a456-426614174001",
                    "published": True,
                },
                "124e4567-e89b-12d3-a456-426614174001": {
                    "id": "124e4567-e89b-12d3-a456-426614174001",
                    "title": "Test Post 2",
                    "content": "Content 2",
                    "authorId": "323e4567-e89b-12d3-a456-426614174002",
                    "published": True,
                },
            }

            self.comments = [
                {
                    "id": "423e4567-e89b-12d3-a456-426614174003",
                    "postId": "123e4567-e89b-12d3-a456-426614174000",
                    "authorId": "323e4567-e89b-12d3-a456-426614174002",
                    "content": "Great post!",
                    "parentCommentId": None,
                },
                {
                    "id": "424e4567-e89b-12d3-a456-426614174004",
                    "postId": "124e4567-e89b-12d3-a456-426614174001",
                    "authorId": "223e4567-e89b-12d3-a456-426614174001",
                    "content": "Nice work!",
                    "parentCommentId": None,
                },
            ]

            # Track batch calls for testing
            self.batch_calls = []

        async def get_users_by_ids(self, user_ids: list[str]) -> list[dict]:
            self.batch_calls.append(("users", user_ids))
            return [self.users[uid] for uid in user_ids if uid in self.users]

        async def get_posts_by_ids(self, post_ids: list[str]) -> list[dict]:
            self.batch_calls.append(("posts", post_ids))
            return [self.posts[pid] for pid in post_ids if pid in self.posts]

        async def get_comments_by_post_ids(self, post_ids: list[str]) -> list[dict]:
            self.batch_calls.append(("comments", post_ids))
            return [c for c in self.comments if c["postId"] in post_ids]

    return MockBlogDB()


def test_dataloader_classes_exist() -> None:
    """Test that all DataLoader classes are properly defined."""
    # Test that we can instantiate the DataLoaders
    mock_db = Mock()

    user_loader = UserDataLoader(mock_db)
    assert user_loader is not None

    comments_loader = CommentsByPostDataLoader(mock_db)
    assert comments_loader is not None

    post_loader = PostDataLoader(mock_db)
    assert post_loader is not None


@pytest.mark.asyncio
async def test_user_dataloader_batching(mock_blog_db) -> None:
    """Test that UserDataLoader properly batches requests."""
    loader = UserDataLoader(mock_blog_db)

    # Load multiple users - should batch into single call
    user_ids = [
        UUID("223e4567-e89b-12d3-a456-426614174001"),
        UUID("323e4567-e89b-12d3-a456-426614174002"),
    ]

    # Load them concurrently (simulating GraphQL field resolution)
    import asyncio

    users = await asyncio.gather(*[loader.load(uid) for uid in user_ids])

    # Should have made exactly one batch call
    assert len(mock_blog_db.batch_calls) == 1
    assert mock_blog_db.batch_calls[0][0] == "users"

    # Should have returned both users
    assert len(users) == 2
    assert users[0]["name"] == "John Doe"
    assert users[1]["name"] == "Jane Smith"


@pytest.mark.asyncio
async def test_comments_dataloader_batching(mock_blog_db) -> None:
    """Test that CommentsByPostDataLoader properly batches requests."""
    loader = CommentsByPostDataLoader(mock_blog_db)

    # Load comments for multiple posts
    post_ids = [
        UUID("123e4567-e89b-12d3-a456-426614174000"),
        UUID("124e4567-e89b-12d3-a456-426614174001"),
    ]

    # Load them concurrently
    import asyncio

    comments_lists = await asyncio.gather(*[loader.load(pid) for pid in post_ids])

    # Should have made exactly one batch call
    assert len(mock_blog_db.batch_calls) == 1
    assert mock_blog_db.batch_calls[0][0] == "comments"

    # Should have returned comments for each post
    assert len(comments_lists) == 2
    assert len(comments_lists[0]) == 1  # One comment for first post
    assert len(comments_lists[1]) == 1  # One comment for second post


@pytest.mark.asyncio
async def test_dataloader_caching(mock_blog_db) -> None:
    """Test that DataLoader caches results within the same instance."""
    loader = UserDataLoader(mock_blog_db)

    user_id = UUID("223e4567-e89b-12d3-a456-426614174001")

    # Load the same user twice
    user1 = await loader.load(user_id)
    user2 = await loader.load(user_id)

    # Should have made only one batch call due to caching
    assert len(mock_blog_db.batch_calls) == 1

    # Both results should be the same
    assert user1 == user2
    assert user1["name"] == "John Doe"


def test_dataloader_integration_with_get_loader() -> None:
    """Test that DataLoaders work with the get_loader function."""

    # Create test types
    @fraiseql.type
    class SampleUser:
        id: UUID
        name: str
        email: str

    @fraiseql.query
    async def test_loader_query(info) -> str:
        """Test query that uses get_loader."""
        try:
            # This should work if LoaderRegistry is properly set up
            user_loader = get_loader(UserDataLoader)
            return f"Success: {type(user_loader).__name__}"
        except Exception as e:
            return f"Error: {e!s}"

    # Create app and test
    app = create_fraiseql_app(database_url="postgresql://test/test", types=[SampleUser])

    with TestClient(app) as client:
        response = client.post("/graphql", json={"query": "{ testLoaderQuery }"})

        assert response.status_code == 200
        data = response.json()

        # Should successfully get a DataLoader
        result = data["data"]["testLoaderQuery"]
        assert "Success" in result
        assert "UserDataLoader" in result


def test_blog_example_no_longer_has_n_plus_one() -> None:
    """Test that the blog example field resolvers use DataLoader."""
    # Import the updated resolvers
    # Check that they import get_loader (indicates DataLoader usage)
    import inspect

    from queries import resolve_comment_author, resolve_post_author, resolve_post_comments

    # Check resolve_post_author source
    source = inspect.getsource(resolve_post_author)
    assert "get_loader" in source
    assert "UserDataLoader" in source
    assert "DataLoader" in source.__doc__ or "N+1" in source

    # Check resolve_comment_author source
    source = inspect.getsource(resolve_comment_author)
    assert "get_loader" in source
    assert "UserDataLoader" in source

    # Check resolve_post_comments source
    source = inspect.getsource(resolve_post_comments)
    assert "get_loader" in source
    assert "CommentsByPostDataLoader" in source
