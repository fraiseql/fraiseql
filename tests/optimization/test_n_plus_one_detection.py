"""Tests for N+1 query detection."""

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
    authorId: UUID  # noqa: N815

    @fraiseql.field
    async def author(self, info) -> Author | None:
        """Simulate a database query for author."""
        # This simulates a database query - in real app would hit DB
        # For testing, just return mock data
        return Author(
            id=self.authorId,
            name=f"Author {self.authorId}",
            email=f"author-{self.authorId}@example.com",
        )


# Query that will trigger N+1
@fraiseql.query
async def get_articles(info) -> list[Article]:
    """Get multiple articles - this will trigger N+1 when fetching authors."""
    # Return 15 articles with different authors
    articles = []
    for i in range(15):
        authorId = uuid4()
        articles.append(
            Article(
                id=uuid4(),
                title=f"Article {i}",
                content=f"Content for article {i}",
                authorId=authorId,
            ),
        )
    return articles


# Query that won't trigger N+1 (single item)
@fraiseql.query
async def get_article(info, id: UUID) -> Article | None:
    """Get a single article."""
    return Article(
        id=id,
        title="Single Article",
        content="This is a single article",
        authorId=uuid4(),
    )


def test_n1_detection_triggers_warning(caplog) -> None:
    """Test that N+1 queries trigger warnings in development mode."""
    # Configure detector with low threshold for testing
    configure_detector(threshold=10, enabled=True, raise_on_detection=False)

    app = create_fraiseql_app(
        database_url="postgresql://fraiseql:fraiseql@localhost:5433/fraiseql_demo",
        types=[Author, Article],
        queries=[get_articles, get_article],
        production=False,  # Development mode enables N+1 detection
    )

    with TestClient(app) as client:
        # Set log level to capture warnings
        with caplog.at_level(logging.WARNING):
            # Query that triggers N+1
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        query {
                            getArticles {
                                id
                                title
                                author {
                                    id
                                    name
                                    email
                                }
                            }
                        }
                    """,
                },
            )

            assert response.status_code == 200
            data = response.json()

            # Query should succeed
            assert "data" in data
            assert len(data["data"]["getArticles"]) == 15

            # Check for N+1 warning in logs
            warning_found = any(
                "N+1 query pattern detected" in record.message for record in caplog.records
            )
            assert warning_found, "N+1 detection warning not found in logs"

            # Check for specific suggestion
            suggestion_found = any(
                "Consider using a DataLoader" in record.message for record in caplog.records
            )
            assert suggestion_found, "DataLoader suggestion not found in logs"


def test_n1_detection_with_raise_enabled() -> None:
    """Test that N+1 detection can raise exceptions when configured."""
    app = create_fraiseql_app(
        database_url="postgresql://fraiseql:fraiseql@localhost:5433/fraiseql_demo",
        types=[Author, Article],
        queries=[get_articles, get_article],
        production=False,
    )

    # Configure detector to raise on detection AFTER app creation
    configure_detector(threshold=10, enabled=True, raise_on_detection=True)

    with TestClient(app) as client:
        # Query that triggers N+1
        response = client.post(
            "/graphql",
            json={
                "query": """
                    query {
                        getArticles {
                            id
                            title
                            author {
                                id
                                name
                            }
                        }
                    }
                """,
            },
        )

        # With raise_on_detection=True, the query should fail
        assert response.status_code == 200
        data = response.json()

        # Should have errors due to N+1 detection
        assert "errors" in data
        assert any(
            "N+1 query pattern detected" in error.get("message", "")
            for error in data.get("errors", [])
        )


def test_n1_detection_respects_threshold() -> None:
    """Test that N+1 detection respects the configured threshold."""
    # Configure with high threshold
    configure_detector(
        threshold=20,  # Higher than our 15 articles
        enabled=True,
        raise_on_detection=False,
    )

    app = create_fraiseql_app(
        database_url="postgresql://fraiseql:fraiseql@localhost:5433/fraiseql_demo",
        types=[Author, Article],
        queries=[get_articles, get_article],
        production=False,
    )

    with TestClient(app) as client:
        # Query with 15 items (below threshold)
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
                """,
            },
        )

        assert response.status_code == 200
        data = response.json()

        # Should succeed without N+1 warnings
        assert "data" in data
        assert "errors" not in data


def test_n1_detection_disabled_in_production() -> None:
    """Test that N+1 detection is disabled in production mode."""
    app = create_fraiseql_app(
        database_url="postgresql://fraiseql:fraiseql@localhost:5433/fraiseql_demo",
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
                """,
            },
        )

        assert response.status_code == 200
        data = response.json()

        # Should succeed without any N+1 detection
        assert "data" in data
        assert "errors" not in data


def test_n1_detection_single_query_no_warning(caplog) -> None:
    """Test that single queries don't trigger N+1 warnings."""
    configure_detector(
        threshold=2,  # Very low threshold
        enabled=True,
        raise_on_detection=False,
    )

    app = create_fraiseql_app(
        database_url="postgresql://fraiseql:fraiseql@localhost:5433/fraiseql_demo",
        types=[Author, Article],
        queries=[get_articles, get_article],
        production=False,
    )

    with TestClient(app) as client, caplog.at_level(logging.WARNING):
        # Query single article
        article_id = uuid4()
        response = client.post(
            "/graphql",
            json={
                "query": f"""
                        query {{
                            getArticle(id: "{article_id}") {{
                                id
                                title
                                author {{
                                    name
                                }}
                            }}
                        }}
                    """,
            },
        )

        assert response.status_code == 200

        # No N+1 warning should be logged
        warning_found = any(
            "N+1 query pattern detected" in record.message for record in caplog.records
        )
        assert not warning_found


def test_field_decorator_without_n1_tracking() -> None:
    """Test that field decorator can disable N+1 tracking."""

    @fraiseql.type
    class Product:
        id: UUID
        name: str

        @fraiseql.field(track_n1=False)  # Disable N+1 tracking
        async def expensive_calculation(self, info) -> str:
            """This won't be tracked for N+1."""
            return f"Expensive result for {self.name}"

    @fraiseql.query
    async def get_products(info) -> list[Product]:
        return [Product(id=uuid4(), name=f"Product {i}") for i in range(20)]

    configure_detector(threshold=5, enabled=True, raise_on_detection=True)

    app = create_fraiseql_app(
        database_url="postgresql://fraiseql:fraiseql@localhost:5433/fraiseql_demo",
        types=[Product],
        queries=[get_products],
        production=False,
    )

    with TestClient(app) as client:
        # This should not trigger N+1 detection
        response = client.post(
            "/graphql",
            json={
                "query": """
                    query {
                        getProducts {
                            id
                            name
                            expensiveCalculation
                        }
                    }
                """,
            },
        )

        assert response.status_code == 200
        data = response.json()

        # Should succeed without N+1 errors
        assert "data" in data
        assert "errors" not in data
        assert len(data["data"]["getProducts"]) == 20
