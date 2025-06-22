"""Tests for N+1 query detection.

🚀 Uses FraiseQL's UNIFIED CONTAINER system - see database_conftest.py
These tests use real database connections for authentic N+1 detection.
"""

import logging
from uuid import UUID, uuid4

import psycopg
import pytest
from fastapi.testclient import TestClient

import fraiseql
from fraiseql.db import DatabaseQuery, FraiseQLRepository
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.gql.schema_builder import SchemaRegistry
from fraiseql.optimization import configure_detector
from psycopg.sql import SQL


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


async def setup_n1_test_tables(conn: psycopg.AsyncConnection) -> None:
    """Create test tables for N+1 detection tests."""
    # Create authors table
    await conn.execute(
        """
        CREATE TABLE IF NOT EXISTS n1_authors (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            data JSONB NOT NULL DEFAULT '{}'::jsonb
        )
    """,
    )

    # Create articles table
    await conn.execute(
        """
        CREATE TABLE IF NOT EXISTS n1_articles (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            author_id UUID NOT NULL REFERENCES n1_authors(id),
            data JSONB NOT NULL DEFAULT '{}'::jsonb
        )
    """,
    )

    # Insert test authors
    author_ids = []
    for i in range(5):
        result = await conn.execute(
            """
            INSERT INTO n1_authors (data) 
            VALUES (%s) 
            RETURNING id
            """,
            (psycopg.types.json.Json({"name": f"Author {i}", "email": f"author{i}@example.com"}),),
        )
        row = await result.fetchone()
        author_ids.append(row[0])

    # Insert articles (3 per author = 15 total)
    for i, author_id in enumerate(author_ids):
        for j in range(3):
            await conn.execute(
                """
                INSERT INTO n1_articles (author_id, data) 
                VALUES (%s, %s)
                """,
                (
                    author_id,
                    psycopg.types.json.Json({"title": f"Article {i}-{j}", "content": f"Content for article {i}-{j}"}),
                ),
            )

    await conn.commit()


async def cleanup_n1_test_tables(conn: psycopg.AsyncConnection) -> None:
    """Clean up test tables."""
    await conn.execute("DROP TABLE IF EXISTS n1_articles CASCADE")
    await conn.execute("DROP TABLE IF EXISTS n1_authors CASCADE")
    await conn.commit()


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
        """Fetch author from database - this will trigger N+1 if called in a loop."""
        repository: FraiseQLRepository = info.context["db"]
        
        query = DatabaseQuery(
            statement=SQL(
                "SELECT id, data->>'name' as name, data->>'email' as email "
                "FROM n1_authors WHERE id = %s"
            ),
            params=(self.authorId,),
            fetch_result=True,
        )
        
        result = await repository.run(query)
        if result:
            row = result[0]
            return Author(
                id=row["id"],
                name=row["name"],
                email=row["email"],
            )
        return None


# Query that will trigger N+1
@fraiseql.query
async def get_articles(info) -> list[Article]:
    """Get multiple articles - this will trigger N+1 when fetching authors."""
    repository: FraiseQLRepository = info.context["db"]
    
    query = DatabaseQuery(
        statement=SQL(
            "SELECT id, author_id, data->>'title' as title, data->>'content' as content "
            "FROM n1_articles ORDER BY id"
        ),
        params={},
        fetch_result=True,
    )
    
    result = await repository.run(query)
    articles = []
    for row in result:
        articles.append(
            Article(
                id=row["id"],
                title=row["title"],
                content=row["content"],
                authorId=row["author_id"],
            ),
        )
    return articles


# Query that won't trigger N+1 (single item)
@fraiseql.query
async def get_article(info, id: UUID) -> Article | None:
    """Get a single article."""
    repository: FraiseQLRepository = info.context["db"]
    
    query = DatabaseQuery(
        statement=SQL(
            "SELECT id, author_id, data->>'title' as title, data->>'content' as content "
            "FROM n1_articles WHERE id = %s"
        ),
        params=(id,),
        fetch_result=True,
    )
    
    result = await repository.run(query)
    if result:
        row = result[0]
        return Article(
            id=row["id"],
            title=row["title"],
            content=row["content"],
            authorId=row["author_id"],
        )
    return None


@pytest.mark.database
@pytest.mark.asyncio
async def test_n1_detection_triggers_warning(db_pool, postgres_url, caplog) -> None:
    """Test that N+1 queries trigger warnings in development mode."""
    # Setup test data
    async with db_pool.connection() as conn:
        await setup_n1_test_tables(conn)

    try:
        # Configure detector with low threshold for testing
        configure_detector(threshold=10, enabled=True, raise_on_detection=False)

        # Create app with database
        app = create_fraiseql_app(
            types=[Author, Article],
            queries=[get_articles, get_article],
            production=False,  # Development mode enables N+1 detection
            database_url=postgres_url,
        )

        with TestClient(app) as client, caplog.at_level(logging.WARNING):
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
    finally:
        # Cleanup
        async with db_pool.connection() as conn:
            await cleanup_n1_test_tables(conn)


@pytest.mark.database
@pytest.mark.asyncio
async def test_n1_detection_with_raise_enabled(db_pool, postgres_url) -> None:
    """Test that N+1 detection can raise exceptions when configured."""
    # Setup test data
    async with db_pool.connection() as conn:
        await setup_n1_test_tables(conn)

    try:
        # Create app with database
        app = create_fraiseql_app(
            types=[Author, Article],
            queries=[get_articles, get_article],
            production=False,
            database_url=postgres_url,
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
    finally:
        # Cleanup
        async with db_pool.connection() as conn:
            await cleanup_n1_test_tables(conn)


@pytest.mark.database
@pytest.mark.asyncio
async def test_n1_detection_respects_threshold(db_pool, postgres_url) -> None:
    """Test that N+1 detection respects the configured threshold."""
    # Setup test data
    async with db_pool.connection() as conn:
        await setup_n1_test_tables(conn)

    try:
        # Configure with high threshold
        configure_detector(
            threshold=20,  # Higher than our 15 articles
            enabled=True,
            raise_on_detection=False,
        )

        # Create app with database
        app = create_fraiseql_app(
            types=[Author, Article],
            queries=[get_articles, get_article],
            production=False,
            database_url=postgres_url,
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
    finally:
        # Cleanup
        async with db_pool.connection() as conn:
            await cleanup_n1_test_tables(conn)


@pytest.mark.database
@pytest.mark.asyncio
async def test_n1_detection_disabled_in_production(db_pool, postgres_url) -> None:
    """Test that N+1 detection is disabled in production mode."""
    # Setup test data
    async with db_pool.connection() as conn:
        await setup_n1_test_tables(conn)

    try:
        # Create app with database
        app = create_fraiseql_app(
            types=[Author, Article],
            queries=[get_articles, get_article],
            production=True,  # Production mode
            database_url=postgres_url,
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

            # Check response
            if response.status_code != 200:
                print(f"Response status: {response.status_code}")
                print(f"Response headers: {response.headers}")
                print(f"Response text: {response.text}")
            
            assert response.status_code == 200
            
            # Handle potential null response
            try:
                data = response.json()
            except Exception as e:
                print(f"Failed to parse JSON: {e}")
                print(f"Response text: {response.text}")
                raise
            
            # Debug null response
            if data is None:
                print(f"Response returned null. Status: {response.status_code}")
                print(f"Response headers: {response.headers}")
                print(f"Response text: {response.text}")
                # Check if there's a GraphQL endpoint issue
                test_response = client.get("/graphql")
                print(f"GET /graphql status: {test_response.status_code}")
            
            # Should succeed without any N+1 detection
            assert data is not None, f"Response data is None. Response text: {response.text}"
            assert "data" in data
            assert "errors" not in data
    finally:
        # Cleanup
        async with db_pool.connection() as conn:
            await cleanup_n1_test_tables(conn)


@pytest.mark.database
@pytest.mark.asyncio
async def test_n1_detection_single_query_no_warning(db_pool, postgres_url, caplog) -> None:
    """Test that single queries don't trigger N+1 warnings."""
    # Setup test data
    async with db_pool.connection() as conn:
        await setup_n1_test_tables(conn)
        # Get a real article ID
        result = await conn.execute("SELECT id FROM n1_articles LIMIT 1")
        row = await result.fetchone()
        article_id = row[0]

    try:
        configure_detector(
            threshold=2,  # Very low threshold
            enabled=True,
            raise_on_detection=False,
        )

        # Create app with database
        app = create_fraiseql_app(
            types=[Author, Article],
            queries=[get_articles, get_article],
            production=False,
            database_url=postgres_url,
        )

        with TestClient(app) as client, caplog.at_level(logging.WARNING):
            # Query single article
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
    finally:
        # Cleanup
        async with db_pool.connection() as conn:
            await cleanup_n1_test_tables(conn)


@pytest.mark.database
@pytest.mark.asyncio
async def test_field_decorator_without_n1_tracking(db_pool, postgres_url) -> None:
    """Test that field decorator can disable N+1 tracking."""
    # Setup products table
    async with db_pool.connection() as conn:
        await conn.execute(
            """
            CREATE TABLE IF NOT EXISTS n1_products (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                data JSONB NOT NULL DEFAULT '{}'::jsonb
            )
        """,
        )
        # Insert test products
        for i in range(20):
            await conn.execute(
                "INSERT INTO n1_products (data) VALUES (%s)",
                (psycopg.types.json.Json({"name": f"Product {i}"}),),
            )
        await conn.commit()

    try:
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
            repository: FraiseQLRepository = info.context["db"]
            
            query = DatabaseQuery(
                statement=SQL(
                    "SELECT id, data->>'name' as name FROM n1_products ORDER BY id"
                ),
                params={},
                fetch_result=True,
            )
            
            result = await repository.run(query)
            return [Product(id=row["id"], name=row["name"]) for row in result]

        configure_detector(threshold=5, enabled=True, raise_on_detection=True)

        # Create app with database
        app = create_fraiseql_app(
            types=[Product],
            queries=[get_products],
            production=False,
            database_url=postgres_url,
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
    finally:
        # Cleanup
        async with db_pool.connection() as conn:
            await conn.execute("DROP TABLE IF EXISTS n1_products CASCADE")
            await conn.commit()
