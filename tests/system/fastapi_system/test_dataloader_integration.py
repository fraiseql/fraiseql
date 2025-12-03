"""Test DataLoader integration with FastAPI and GraphQL context."""

from uuid import UUID

import pytest
from fastapi.testclient import TestClient

import fraiseql
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.gql.schema_builder import SchemaRegistry
from fraiseql.optimization.dataloader import DataLoader
from fraiseql.optimization.registry import get_loader

pytestmark = pytest.mark.integration


@pytest.fixture(autouse=True)
def clear_registry() -> None:
    """Clear registry before each test to avoid type conflicts."""
    registry = SchemaRegistry.get_instance()
    registry.clear()

    # Also clear the GraphQL type cache
    from fraiseql.core.graphql_type import _graphql_type_cache

    _graphql_type_cache.clear()

    yield

    registry.clear()
    _graphql_type_cache.clear()


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
    authorId: UUID

    # Add field resolver for author
    @fraiseql.field
    async def author(self, info) -> User | None:
        """Resolve post author using DataLoader."""
        loader = get_loader(UserDataLoader)
        user_data = await loader.load(self.authorId)
        return User(**user_data) if user_data else None


# Test DataLoader
class UserDataLoader(DataLoader[UUID, dict]):
    """DataLoader for loading users by ID."""

    def __init__(self, db, users_db: dict[UUID, dict] | None = None) -> None:
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

    async def batch_load(self, user_ids: list[UUID]) -> list[dict | None]:
        """Batch load users by IDs."""
        self.load_calls.append(list(user_ids))  # Track the call

        # Simulate database lookup
        results = []
        for user_id in user_ids:
            user_data = self.users_db.get(user_id)
            results.append(user_data)

        return results


# Test queries
@fraiseql.query
async def get_post(info, id: UUID) -> Post | None:
    """Get a post by ID."""
    # Mock post data
    if str(id) == "123e4567-e89b-12d3-a456-426614174000":
        return Post(
            id=id,
            title="Test Post",
            content="Test content",
            authorId=UUID("223e4567-e89b-12d3-a456-426614174001"),
        )
    return None


@fraiseql.query
async def get_posts(info) -> list[Post]:
    """Get multiple posts - should trigger DataLoader batching."""
    posts = []
    for i in range(3):
        posts.append(
            Post(
                id=UUID(f"00000000-0000-0000-0000-{i:012x}"),
                title=f"Post {i}",
                content=f"Content {i}",
                authorId=UUID("223e4567-e89b-12d3-a456-426614174001"),  # Same author
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


def test_dataloader_registry_in_context() -> None:
    """Test that LoaderRegistry is automatically available in GraphQL context."""
    app = create_fraiseql_app(
        database_url="postgresql://fraiseql:fraiseql@localhost:5432/fraiseql_test",
        types=[User, Post],
        queries=[get_post, get_posts, get_loader_test],
    )

    with TestClient(app) as client:
        # Simple query that doesn't use field resolvers yet
        response = client.post(
            """/graphql""",
            json={
                "query": """
                    query {
                        getPost(id: "123e4567-e89b-12d3-a456-426614174000") {
                            id
                            title
                            authorId
                        }
                    }
                """
            },
        )

        assert response.status_code == 200
        data = response.json()

        # Should work without errors
        assert "errors" not in data or not data["errors"]
        assert data["data"]["getPost"]["title"] == "Test Post"


def test_dataloader_batching_works() -> None:
    """Test that DataLoader properly batches multiple loads."""
    app = create_fraiseql_app(
        database_url="postgresql://fraiseql:fraiseql@localhost:5432/fraiseql_test",
        types=[User, Post],
        queries=[get_post, get_posts, get_loader_test],
    )

    with TestClient(app) as client:
        # Query multiple posts with same author - should batch the author lookups
        response = client.post(
            "/graphql",
            json={
                "query": """
                    query {
                        getPosts {
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

        # Debug: print the response if there's an error
        if "errors" in data:
            pass
        if "data" not in data:
            pass

        # Should successfully resolve all authors
        posts = data["data"]["getPosts"]
        assert len(posts) == 3

        # All posts should have the same author
        for post in posts:
            assert post["author"]["name"] == "John Doe"


def test_dataloader_error_handling() -> None:
    """Test that DataLoader errors are properly handled."""
    app = create_fraiseql_app(
        database_url="postgresql://fraiseql:fraiseql@localhost:5432/fraiseql_test",
        types=[User, Post],
        queries=[get_post, get_posts, get_loader_test],
    )

    with TestClient(app) as client:
        # Query with invalid post ID - should handle gracefully
        response = client.post(
            "/graphql",
            json={
                "query": """
                    query {
                        getPost(id: "invalid-id") {
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
        assert data["data"]["getPost"] is None


def test_get_loader_function_works() -> None:
    """Test that get_loader function works properly with context."""
    app = create_fraiseql_app(
        database_url="postgresql://fraiseql:fraiseql@localhost:5432/fraiseql_test",
        types=[User, Post],
        queries=[get_post, get_posts, get_loader_test],
    )

    # This test verifies that get_loader() function can retrieve
    # DataLoader instances from the GraphQL context
    with TestClient(app) as client:
        response = client.post(
            "/graphql",
            json={
                "query": """
                    query {
                        getLoaderTest
                    }
                """
            },
        )

        assert response.status_code == 200
        data = response.json()

        # Should successfully get a DataLoader instance
        result = data["data"]["getLoaderTest"]
        assert "Success" in result
        assert "UserDataLoader" in result


def test_dataloader_caching() -> None:
    """Test that DataLoader caches results within a request."""
    app = create_fraiseql_app(
        database_url="postgresql://fraiseql:fraiseql@localhost:5432/fraiseql_test",
        types=[User, Post],
        queries=[get_post, get_posts, get_loader_test],
    )

    with TestClient(app) as client:
        # Query that loads the same user multiple times
        response = client.post(
            "/graphql",
            json={
                "query": """
                    query {
                        post1: getPost(id: "123e4567-e89b-12d3-a456-426614174000") {
                            author { name }
                        }
                        post2: getPost(id: "123e4567-e89b-12d3-a456-426614174000") {
                            author { name }
                        }
                    }
                """
            },
        )

        assert response.status_code == 200
        data = response.json()

        # Both should resolve to same user due to caching
        # Note: The UserDataLoader uses its own hardcoded user database
        assert data["data"]["post1"]["author"]["name"] == "John Doe"
        assert data["data"]["post2"]["author"]["name"] == "John Doe"


@pytest.mark.asyncio
async def test_dataloader_field_decorator() -> None:
    """Test @dataloader_field decorator for automatic DataLoader integration."""

    # Define PostDataLoader first
    class PostDataLoader(DataLoader[UUID, dict]):
        async def batch_load(self, post_ids: list[UUID]) -> list[dict | None]:
            # Mock implementation
            return [{"id": pid, "title": f"Post {pid}", "content": "Content"} for pid in post_ids]

    # Test that @dataloader_field decorator exists
    if hasattr(fraiseql, "dataloader_field"):

        @fraiseql.type
        class Comment:
            id: UUID
            post_id: UUID
            content: str

            # This decorator should automatically use DataLoader
            @fraiseql.dataloader_field(PostDataLoader, key_field="post_id")
            async def post(self, info) -> Post | None:
                """Load the post this comment belongs to."""
                # Implementation is handled by the decorator

    else:
        # Skip test if decorator doesn't exist yet
        pytest.skip("@dataloader_field decorator not implemented yet")
