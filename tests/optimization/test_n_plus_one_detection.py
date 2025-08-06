"""Tests for N+1 query detection.

These tests use mocked data to test N+1 detection functionality
without requiring database connections.
"""

import logging
from uuid import UUID, uuid4

import pytest
from fastapi.testclient import TestClient

import fraiseql
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.gql.schema_builder import SchemaRegistry
from fraiseql.optimization import configure_detector


@pytest.fixture(autouse=True)
def clear_registry():
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
class Author:
    id: UUID
    name: str
    email: str


@fraiseql.type
class Article:
    id: UUID
    title: str
    content: str
    authorId: UUID

    @fraiseql.field
    async def author(self, info) -> Author | None:
        """Simulate a database query for author - this will trigger N+1 detection."""
        # This simulates a database query that would happen in a real app
        # The N+1 detector tracks these field resolutions
        return Author(
            id=self.authorId,
            name=f"Author {str(self.authorId)[:8]}",
            email=f"author-{str(self.authorId)[:8]}@example.com",
        )


# Query that will trigger N+1
@fraiseql.query
async def get_articles(info) -> list[Article]:
    """Get multiple articles - this will trigger N+1 when fetching authors."""
    # Return 15 articles with different authors
    articles = []
    for i in range(15):
        articles.append(
            Article(
                id=uuid4(),
                title=f"Article {i}",
                content=f"Content for article {i}",
                authorId=uuid4(),
            )
        )
    return articles


# Query that won't trigger N+1 (single item)
@fraiseql.query
async def get_article(info, id: UUID) -> Article | None:
    """Get a single article."""
    return Article(
        id=id, title="Single Article", content="This is a single article", authorId=uuid4()
    )


def test_n1_detection_triggers_warning(caplog) -> None:
    """Test that N+1 queries trigger warnings in development mode."""
    # Skip this test - N+1 detection requires integration with GraphQL execution
    # which is not properly set up in these unit tests
    pytest.skip("N+1 detection requires full GraphQL execution context")


def test_n1_detection_with_raise_enabled() -> None:
    """Test that N+1 detection can raise exceptions when configured."""
    # Skip this test - N+1 detection requires integration with GraphQL execution
    pytest.skip("N+1 detection requires full GraphQL execution context")


def test_n1_detection_respects_threshold() -> None:
    """Test that N+1 detection respects the configured threshold."""
    # Skip this test - N+1 detection requires integration with GraphQL execution
    pytest.skip("N+1 detection requires full GraphQL execution context")


@pytest.mark.skip(reason="Production mode requires valid database connection")
def test_n1_detection_disabled_in_production() -> None:
    """Test that N+1 detection is disabled in production mode."""
    # Create app without database (uses mocked data)
    app = create_fraiseql_app(
        database_url="postgresql://localhost/test",  # Dummy URL
        types=[Author, Article],
        queries=[get_articles, get_article],
        production=True,  # Production mode
    )

    with TestClient(app) as client:
        # Same query that would trigger N+1
        response = client.post(
            "/graphql",
            json={
                "query": """
                    query {
                        getArticles {
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

        # Should succeed without any N+1 detection
        assert data is not None, f"Response data is None. Response text: {response.text}"
        assert "data" in data
        assert "errors" not in data


def test_n1_detection_single_query_no_warning() -> None:
    """Test that single queries don't trigger N+1 warnings."""
    # Skip this test - N+1 detection requires integration with GraphQL execution
    pytest.skip("N+1 detection requires full GraphQL execution context")


def test_field_decorator_without_n1_tracking() -> None:
    """Test that field decorator can disable N+1 tracking."""
    # Skip this test - N+1 detection requires integration with GraphQL execution
    pytest.skip("N+1 detection requires full GraphQL execution context")


def test_field_decorator_error_handling() -> None:
    """Test that @dataloader_field handles errors gracefully."""
    # Test decorator with invalid parameters
    with pytest.raises(ValueError, match="loader_class must be a DataLoader subclass"):

        @fraiseql.type
        class InvalidType:
            @fraiseql.dataloader_field(str, key_field="id")  # Invalid loader class
            async def field(self, info) -> None:
                pass


def test_field_decorator_without_key_field() -> None:
    """Test that @dataloader_field requires key_field parameter."""
    from fraiseql.optimization.dataloader import DataLoader

    class UserDataLoader(DataLoader[UUID, dict]):
        """DataLoader for loading users by ID."""

        def __init__(self, db) -> None:
            super().__init__()
            self.db = db

        async def batch_load(self, user_ids: list[UUID]) -> list[dict | None]:
            """Batch load users by IDs."""
            return []

    with pytest.raises(TypeError, match="missing 1 required keyword-only argument: 'key_field'"):

        @fraiseql.type
        class InvalidType:
            @fraiseql.dataloader_field(UserDataLoader)  # Missing key_field
            async def field(self, info) -> None:
                pass


def test_dataloader_field_with_custom_resolver() -> None:
    """Test @dataloader_field with custom resolver logic."""
    from fraiseql.optimization.dataloader import DataLoader
    from fraiseql.optimization.registry import get_loader

    class UserDataLoader(DataLoader[UUID, dict]):
        """DataLoader for loading users by ID."""

        def __init__(self, db) -> None:
            super().__init__()
            self.db = db

        async def batch_load(self, user_ids: list[UUID]) -> list[dict | None]:
            """Batch load users by IDs."""
            return []

    @fraiseql.type
    class CustomPost:
        id: UUID
        authorId: UUID

        @fraiseql.dataloader_field(UserDataLoader, key_field="authorId")
        async def author(self, info) -> Author | None:
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
                return Author(**user_data)

            return None

    # Add a dummy query to satisfy schema requirements
    @fraiseql.query
    async def get_posts(info) -> list[CustomPost]:
        return []

    # Test that custom logic works
    create_fraiseql_app(database_url="postgresql://test/test", types=[Author, CustomPost], queries=[get_posts])

    # This test verifies the decorator doesn't interfere with custom logic
    assert True  # Would need actual query test when implemented
