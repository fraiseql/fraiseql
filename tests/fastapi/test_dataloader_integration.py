"""Test DataLoader integration with FastAPI and GraphQL context."""

from typing import Dict, List, Optional
from uuid import UUID

import pytest
from fastapi.testclient import TestClient

import fraiseql
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.optimization.dataloader import DataLoader
from fraiseql.optimization.registry import get_loader


# Test types
@fraiseql.type
class User:
    id: UUID
    name: str
    email: str


@fraiseql.type
class Post:
    id: UUID
    title: str
    content: str
    author_id: UUID


# Test DataLoader
class UserDataLoader(DataLoader[UUID, Dict]):
    """DataLoader for loading users by ID."""

    def __init__(self, db, users_db: Dict[UUID, Dict] = None):
        super().__init__()
        self.db = db
        self.users_db = users_db or {
            UUID("223e4567-e89b-12d3-a456-426614174001"): {
                "id": UUID("223e4567-e89b-12d3-a456-426614174001"),
                "name": "John Doe",
                "email": "john@example.com",
            }
        }
        self.load_calls = []  # Track batch calls for testing

    async def batch_load(self, user_ids: List[UUID]) -> List[Optional[Dict]]:
        """Batch load users by IDs."""
        self.load_calls.append(list(user_ids))  # Track the call

        # Simulate database lookup
        results = []
        for user_id in user_ids:
            user_data = self.users_db.get(user_id)
            results.append(user_data)

        return results


# Field resolver that uses DataLoader
async def resolve_post_author(post: Post, info) -> Optional[User]:
    """Resolve post author using DataLoader."""
    loader = get_loader(UserDataLoader)
    user_data = await loader.load(post.author_id)
    return User(**user_data) if user_data else None


# Test queries
@fraiseql.query
async def get_post(info, id: UUID) -> Optional[Post]:
    """Get a post by ID."""
    # Mock post data
    if str(id) == "123e4567-e89b-12d3-a456-426614174000":
        return Post(
            id=id,
            title="Test Post",
            content="Test content",
            author_id=UUID("223e4567-e89b-12d3-a456-426614174001"),
        )
    return None


@fraiseql.query
async def get_posts(info) -> List[Post]:
    """Get multiple posts - should trigger DataLoader batching."""
    posts = []
    for i in range(3):
        posts.append(
            Post(
                id=UUID(f"{i:032x}-0000-0000-0000-000000000000"),
                title=f"Post {i}",
                content=f"Content {i}",
                author_id=UUID("223e4567-e89b-12d3-a456-426614174001"),  # Same author
            )
        )
    return posts


@fraiseql.query
async def get_loader_test(info) -> str:
    """Test query to verify get_loader works."""
    try:
        # Try to get a DataLoader - this should work if LoaderRegistry is in context
        loader = get_loader(UserDataLoader)
        return f"Success: Got loader {type(loader).__name__}"
    except Exception as e:
        return f"Error: {e!s}"


def test_dataloader_registry_in_context():
    """Test that LoaderRegistry is automatically available in GraphQL context."""
    app = create_fraiseql_app(database_url="postgresql://test/test", types=[User, Post])

    with TestClient(app) as client:
        # Simple query that doesn't use field resolvers yet
        response = client.post(
            "/graphql",
            json={
                "query": """
                    query {
                        get_post(id: "123e4567-e89b-12d3-a456-426614174000") {
                            id
                            title
                            author_id
                        }
                    }
                """
            },
        )

        assert response.status_code == 200
        data = response.json()

        # Should work without errors
        assert "errors" not in data or not data["errors"]
        assert data["data"]["get_post"]["title"] == "Test Post"


def test_dataloader_batching_works():
    """Test that DataLoader properly batches multiple loads."""
    # Mock user database
    users_db = {
        UUID("223e4567-e89b-12d3-a456-426614174001"): {
            "id": UUID("223e4567-e89b-12d3-a456-426614174001"),
            "name": "John Doe",
            "email": "john@example.com",
        }
    }

    app = create_fraiseql_app(database_url="postgresql://test/test", types=[User, Post])

    with TestClient(app) as client:
        # Query multiple posts with same author - should batch the author lookups
        response = client.post(
            "/graphql",
            json={
                "query": """
                    query {
                        get_posts {
                            id
                            title
                            author {
                                id
                                name
                            }
                        }
                    }
                """
            },
        )

        assert response.status_code == 200
        data = response.json()

        # Should successfully resolve all authors
        posts = data["data"]["get_posts"]
        assert len(posts) == 3

        # All posts should have the same author
        for post in posts:
            assert post["author"]["name"] == "John Doe"


def test_dataloader_error_handling():
    """Test that DataLoader errors are properly handled."""
    app = create_fraiseql_app(database_url="postgresql://test/test", types=[User, Post])

    with TestClient(app) as client:
        # Query with invalid post ID - should handle gracefully
        response = client.post(
            "/graphql",
            json={
                "query": """
                    query {
                        get_post(id: "invalid-id") {
                            id
                            author {
                                name
                            }
                        }
                    }
                """
            },
        )

        assert response.status_code == 200
        data = response.json()

        # Should return null for non-existent post, not error
        assert data["data"]["get_post"] is None


def test_get_loader_function_works():
    """Test that get_loader function works properly with context."""
    app = create_fraiseql_app(database_url="postgresql://test/test", types=[User, Post])

    # This test verifies that get_loader() function can retrieve
    # DataLoader instances from the GraphQL context
    with TestClient(app) as client:
        response = client.post(
            "/graphql",
            json={
                "query": """
                    query {
                        get_loader_test
                    }
                """
            },
        )

        assert response.status_code == 200
        data = response.json()

        # Should successfully get a DataLoader instance
        result = data["data"]["get_loader_test"]
        assert "Success" in result
        assert "UserDataLoader" in result


def test_dataloader_caching():
    """Test that DataLoader caches results within a request."""
    users_db = {
        UUID("223e4567-e89b-12d3-a456-426614174001"): {
            "id": UUID("223e4567-e89b-12d3-a456-426614174001"),
            "name": "Cached User",
            "email": "cached@example.com",
        }
    }

    app = create_fraiseql_app(database_url="postgresql://test/test", types=[User, Post])

    with TestClient(app) as client:
        # Query that loads the same user multiple times
        response = client.post(
            "/graphql",
            json={
                "query": """
                    query {
                        post1: get_post(id: "123e4567-e89b-12d3-a456-426614174000") {
                            author { name }
                        }
                        post2: get_post(id: "123e4567-e89b-12d3-a456-426614174000") {
                            author { name }
                        }
                    }
                """
            },
        )

        assert response.status_code == 200
        data = response.json()

        # Both should resolve to same user due to caching
        assert data["data"]["post1"]["author"]["name"] == "Cached User"
        assert data["data"]["post2"]["author"]["name"] == "Cached User"


@pytest.mark.asyncio
async def test_dataloader_field_decorator():
    """Test @dataloader_field decorator for automatic DataLoader integration."""

    @fraiseql.type
    class Comment:
        id: UUID
        post_id: UUID
        content: str

        # This decorator should automatically use DataLoader
        @fraiseql.dataloader_field(PostDataLoader)
        async def post(self, info) -> Optional[Post]:
            """Load the post this comment belongs to."""
            return await self.load_related(self.post_id)

    class PostDataLoader(DataLoader[UUID, Dict]):
        async def batch_load(self, post_ids: List[UUID]) -> List[Optional[Dict]]:
            # Mock implementation
            return [
                {"id": pid, "title": f"Post {pid}", "content": "Content"}
                for pid in post_ids
            ]

    # Test that the decorator works
    # This test will fail until we implement @dataloader_field
    with pytest.raises(AttributeError):
        # Should fail because @dataloader_field doesn't exist yet
        pass


def test_n_plus_one_detection():
    """Test that N+1 query detection works in development mode."""
    app = create_fraiseql_app(
        database_url="postgresql://test/test",
        types=[User, Post],
        production=False,  # Development mode
    )

    # This should detect potential N+1 queries and warn or error
    # This test will be implemented after N+1 detection is added
    pass
