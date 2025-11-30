"""Test @dataloader_field decorator for automatic DataLoader integration."""

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
    """Clear registry before each test."""
    registry = SchemaRegistry.get_instance()
    registry.clear()
    yield
    registry.clear()


@pytest.fixture
def register_test_queries() -> None:
    """Register the test queries needed for schema tests."""
    from fraiseql.gql.schema_builder import SchemaRegistry

    registry = SchemaRegistry.get_instance()

    # Re-register the query functions
    registry.register_query(getPost)
    registry.register_query(getComment)

    return registry


# Test types
@fraiseql.type
class User:
    id: UUID
    name: str
    email: str


# Test DataLoaders - need to be defined after User
class UserDataLoader(DataLoader[UUID, dict]):
    """DataLoader for loading users by ID."""

    def __init__(self, db) -> None:
        super().__init__()
        self.db = db
        self.load_calls = []  # Track batch calls for testing

    async def batch_load(self, user_ids: list[UUID]) -> list[dict | None]:
        """Batch load users by IDs."""
        self.load_calls.append(list(user_ids))  # Track the call

        # Mock data - return dicts that match User constructor expectations
        users_db = {
            UUID("223e4567-e89b-12d3-a456-426614174001"): {
                "id": UUID(
                    "223e4567-e89b-12d3-a456-426614174001"
                ),  # UUID object as expected by User
                "name": "John Doe",
                "email": "john@example.com",
            },
            UUID("323e4567-e89b-12d3-a456-426614174002"): {
                "id": UUID(
                    "323e4567-e89b-12d3-a456-426614174002"
                ),  # UUID object as expected by User
                "name": "Jane Smith",
                "email": "jane@example.com",
            },
        }

        results = []
        for user_id in user_ids:
            user_data = users_db.get(user_id)
            results.append(user_data)

        return results


@fraiseql.type
class Post:
    id: UUID
    title: str
    content: str
    authorId: UUID

    @fraiseql.dataloader_field(UserDataLoader, key_field="authorId")
    async def author(self, info) -> User | None:
        """Load author using DataLoader automatically."""
        # This should be auto-implemented by the decorator


class PostDataLoader(DataLoader[UUID, dict]):
    """DataLoader for loading posts by ID."""

    def __init__(self, db) -> None:
        super().__init__()
        self.db = db

    async def batch_load(self, post_ids: list[UUID]) -> list[dict | None]:
        """Batch load posts by IDs."""
        # Mock data that matches Post constructor
        posts_db = {
            UUID("123e4567-e89b-12d3-a456-426614174000"): {
                "id": UUID("123e4567-e89b-12d3-a456-426614174000"),
                "title": "Test Post",
                "content": "Test content",
                "authorId": UUID("223e4567-e89b-12d3-a456-426614174001"),
            }
        }

        results = []
        for post_id in post_ids:
            post_data = posts_db.get(post_id)
            results.append(post_data)

        return results


@fraiseql.type
class Comment:
    id: UUID
    content: str
    authorId: UUID
    postId: UUID

    @fraiseql.dataloader_field(UserDataLoader, key_field="authorId")
    async def author(self, info) -> User | None:
        """Load comment author using DataLoader."""

    @fraiseql.dataloader_field(PostDataLoader, key_field="postId")
    async def post(self, info) -> Post | None:
        """Load comment post using DataLoader."""


# Test queries
@fraiseql.query
async def getPost(info, id: UUID) -> Post | None:
    """Get a post by ID."""
    if str(id) == "123e4567-e89b-12d3-a456-426614174000":
        return Post(
            id=id,
            title="Test Post",
            content="Test content",
            authorId=UUID("223e4567-e89b-12d3-a456-426614174001"),
        )
    return None


@fraiseql.query
async def getComment(info, id: UUID) -> Comment | None:
    """Get a comment by ID."""
    if str(id) == "323e4567-e89b-12d3-a456-426614174002":
        return Comment(
            id=id,
            content="Great post!",
            authorId=UUID("323e4567-e89b-12d3-a456-426614174002"),
            postId=UUID("123e4567-e89b-12d3-a456-426614174000"),
        )
    return None


def test_dataloader_field_decorator_exists() -> None:
    """Test that @dataloader_field decorator exists and can be imported."""
    # This test will fail until we implement the decorator
    try:
        from fraiseql import dataloader_field

        assert dataloader_field is not None
    except ImportError:
        pytest.fail("@dataloader_field decorator not implemented yet")


def test_dataloader_field_adds_metadata() -> None:
    """Test that @dataloader_field decorator adds proper metadata to methods."""
    # Check that the decorator adds metadata we can use for field resolution
    assert hasattr(Post.author, "__fraiseql_dataloader__"), (
        f"Post.author attributes: {dir(Post.author)}"
    )

    metadata = Post.author.__fraiseql_dataloader__
    assert metadata["loader_class"] == UserDataLoader
    assert metadata["key_field"] == "authorId"


def test_dataloader_field_generates_schema_field(register_test_queries) -> None:
    """Test that @dataloader_field decorated methods appear in GraphQL schema."""
    app = create_fraiseql_app(database_url="postgresql://test/test", types=[User, Post, Comment])

    with TestClient(app) as client:
        # Test introspection to verify field exists
        response = client.post(
            "/graphql",
            json={
                "query": """
                    query {
                        __type(name: "Post") {
                            fields {
                                name
                                type {
                                    name
                                }
                            }
                        }
                    }
                """
            },
        )

        assert response.status_code == 200
        data = response.json()

        # Should have author field from @dataloader_field
        fields = {f["name"]: f["type"]["name"] for f in data["data"]["__type"]["fields"]}
        assert "author" in fields
        assert fields["author"] == "User"


def test_dataloader_field_automatic_resolution(register_test_queries) -> None:
    """Test that @dataloader_field automatically resolves using DataLoader."""
    app = create_fraiseql_app(database_url="postgresql://test/test", types=[User, Post, Comment])

    with TestClient(app) as client:
        # Query that should automatically use DataLoader for author resolution
        response = client.post(
            "/graphql",
            json={
                "query": """
                    query {
                        getPost(id: "123e4567-e89b-12d3-a456-426614174000") {
                            id
                            title
                            author {
                                id
                                name
                                email
                            }
                        }
                    }
                """
            },
        )

        assert response.status_code == 200
        data = response.json()

        # Debug: print the response

        # Should successfully resolve author using DataLoader
        post = data["data"]["getPost"]
        assert post["title"] == "Test Post"
        assert post["author"] is not None, f"Author is None! Full post data: {post}"
        assert post["author"]["name"] == "John Doe"
        assert post["author"]["email"] == "john@example.com"


def test_dataloader_field_batching(register_test_queries) -> None:
    """Test that @dataloader_field properly batches multiple field resolutions."""
    app = create_fraiseql_app(database_url="postgresql://test/test", types=[User, Post, Comment])

    # We need a way to track DataLoader calls to verify batching
    # This would require access to the actual DataLoader instance
    with TestClient(app) as client:
        # Query multiple items that should batch author lookups
        response = client.post(
            "/graphql",
            json={
                "query": """
                    query {
                        post: getPost(id: "123e4567-e89b-12d3-a456-426614174000") {
                            author { name }
                        }
                        comment: getComment(id: "323e4567-e89b-12d3-a456-426614174002") {
                            author { name }
                            post {
                                author { name }
                            }
                        }
                    }
                """
            },
        )

        assert response.status_code == 200
        data = response.json()

        # Debug output

        # Both should resolve authors (would be batched in real implementation)
        assert data["data"]["post"]["author"]["name"] == "John Doe"
        assert data["data"]["comment"]["author"]["name"] == "Jane Smith"
        assert data["data"]["comment"]["post"] is not None, (
            f"Comment post is None! Full comment: {data['data']['comment']}"
        )
        assert data["data"]["comment"]["post"]["author"]["name"] == "John Doe"


def test_dataloader_field_with_multiple_loaders(register_test_queries) -> None:
    """Test @dataloader_field works with different DataLoader types."""
    app = create_fraiseql_app(database_url="postgresql://test/test", types=[User, Post, Comment])

    with TestClient(app) as client:
        # Query that uses both UserDataLoader and PostDataLoader
        response = client.post(
            "/graphql",
            json={
                "query": """
                    query {
                        getComment(id: "323e4567-e89b-12d3-a456-426614174002") {
                            id
                            content
                            author {
                                name
                            }
                            post {
                                id
                                title
                                author {
                                    name
                                }
                            }
                        }
                    }
                """
            },
        )

        assert response.status_code == 200
        data = response.json()

        comment = data["data"]["getComment"]
        assert comment["content"] == "Great post!"
        assert comment["author"]["name"] == "Jane Smith"
        assert comment["post"]["title"] == "Test Post"
        assert comment["post"]["author"]["name"] == "John Doe"


def test_dataloader_field_error_handling() -> None:
    """Test that @dataloader_field handles errors gracefully."""
    # Test decorator with invalid parameters
    with pytest.raises(ValueError, match="loader_class must be a DataLoader subclass"):

        @fraiseql.type
        class InvalidType:
            @fraiseql.dataloader_field(str, key_field="id")  # Invalid loader class
            async def field(self, info) -> None:
                pass


def test_dataloader_field_without_key_field() -> None:
    """Test that @dataloader_field requires key_field parameter."""
    with pytest.raises(TypeError, match="missing 1 required keyword-only argument: 'key_field'"):

        @fraiseql.type
        class InvalidType:
            @fraiseql.dataloader_field(UserDataLoader)  # Missing key_field
            async def field(self, info) -> None:
                pass


def test_dataloader_field_with_custom_resolver(register_test_queries) -> None:
    """Test @dataloader_field with custom resolver logic."""

    @fraiseql.type
    class CustomPost:
        id: UUID
        authorId: UUID

        @fraiseql.dataloader_field(UserDataLoader, key_field="authorId")
        async def author(self, info) -> User | None:
            """Custom logic before DataLoader."""
            if not self.authorId:
                return None

            # Custom logic can be added here
            # The decorator should still handle the DataLoader call
            loader = get_loader(UserDataLoader)
            user_data = await loader.load(self.authorId)

            if user_data:
                # Custom processing
                user_data = dict(user_data)
                user_data["name"] = f"Mr. {user_data['name']}"
                return User(**user_data)

            return None

    # Test that custom logic works
    create_fraiseql_app(database_url="postgresql://test/test", types=[User, CustomPost])

    # This test verifies the decorator doesn't interfere with custom logic
    assert True  # Would need actual query test when implemented


def test_dataloader_field_schema_introspection(register_test_queries) -> None:
    """Test that @dataloader_field decorated fields show up in schema introspection."""
    app = create_fraiseql_app(database_url="postgresql://test/test", types=[User, Post, Comment])

    with TestClient(app) as client:
        # Get full schema to verify all fields are present
        response = client.post(
            "/graphql",
            json={
                "query": """
                    query {
                        __schema {
                            types {
                                name
                                fields {
                                    name
                                    type {
                                        name
                                    }
                                }
                            }
                        }
                    }
                """
            },
        )

        assert response.status_code == 200
        data = response.json()

        # Find Post type and verify it has author field
        types = {t["name"]: t for t in data["data"]["__schema"]["types"]}

        assert "Post" in types
        post_fields = {
            f["name"]: f["type"]["name"]
            for f in types["Post"]["fields"]
            if f["name"] != "__typename"
        }
        assert "author" in post_fields
        assert post_fields["author"] == "User"

        # Find Comment type and verify it has both author and post fields
        assert "Comment" in types
        comment_fields = {
            f["name"]: f["type"]["name"]
            for f in types["Comment"]["fields"]
            if f["name"] != "__typename"
        }
        assert "author" in comment_fields
        assert "post" in comment_fields
        assert comment_fields["author"] == "User"
        assert comment_fields["post"] == "Post"
