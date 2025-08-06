"""Integration tests for JSON passthrough and dual mode execution.

This test suite validates the complete flow from database to HTTP response
for both development mode (object instantiation) and production mode (JSON passthrough).
"""

import json
from decimal import Decimal
from typing import Any, Optional
from uuid import UUID, uuid4

import pytest
from psycopg.sql import SQL

import fraiseql
from fraiseql.db import FraiseQLRepository, register_type_for_view
from fraiseql.core.raw_json_executor import RawJSONResult


# Test types for dual mode testing
@fraiseql.type
class Author:
    id: UUID
    name: str
    email: str
    bio: Optional[str] = None


@fraiseql.type
class Article:
    id: UUID
    title: str
    content: str
    published: bool
    author_id: UUID
    tags: list[str]
    metadata: dict[str, Any]

    # Nested relationship
    author: Optional[Author] = None


class TestJSONPassthroughIntegration:
    """Integration tests for JSON passthrough and dual mode execution."""

    @pytest.fixture
    async def setup_test_schema(self, db_pool):
        """Create test schema with articles and authors."""
        async with db_pool.connection() as conn:
            # Create tables
            await conn.execute(
                """
                CREATE TABLE IF NOT EXISTS test_authors (
                    id UUID PRIMARY KEY,
                    name TEXT NOT NULL,
                    email TEXT NOT NULL,
                    bio TEXT
                )
                """
            )

            await conn.execute(
                """
                CREATE TABLE IF NOT EXISTS test_articles (
                    id UUID PRIMARY KEY,
                    title TEXT NOT NULL,
                    content TEXT NOT NULL,
                    published BOOLEAN NOT NULL DEFAULT false,
                    author_id UUID NOT NULL REFERENCES test_authors(id),
                    tags TEXT[] NOT NULL DEFAULT '{}',
                    metadata JSONB NOT NULL DEFAULT '{}'
                )
                """
            )

            # Create views with JSONB data columns for production mode
            await conn.execute(
                """
                CREATE OR REPLACE VIEW test_authors_view AS
                SELECT
                    id, name, email, bio,
                    jsonb_build_object(
                        'id', id,
                        'name', name,
                        'email', email,
                        'bio', bio
                    ) as data
                FROM test_authors
                """
            )

            await conn.execute(
                """
                CREATE OR REPLACE VIEW test_articles_view AS
                SELECT
                    a.id, a.title, a.content, a.published, a.author_id, a.tags, a.metadata,
                    jsonb_build_object(
                        'id', a.id,
                        'title', a.title,
                        'content', a.content,
                        'published', a.published,
                        'author_id', a.author_id,
                        'tags', a.tags,
                        'metadata', a.metadata,
                        'author', jsonb_build_object(
                            'id', au.id,
                            'name', au.name,
                            'email', au.email,
                            'bio', au.bio
                        )
                    ) as data
                FROM test_articles a
                LEFT JOIN test_authors au ON a.author_id = au.id
                """
            )

            # Insert test data
            author_id = uuid4()
            await conn.execute(
                """
                INSERT INTO test_authors (id, name, email, bio)
                VALUES (%s, %s, %s, %s)
                """,
                (author_id, "Jane Doe", "jane@example.com", "Tech writer and developer")
            )

            article_id = uuid4()
            await conn.execute(
                """
                INSERT INTO test_articles (id, title, content, published, author_id, tags, metadata)
                VALUES (%s, %s, %s, %s, %s, %s, %s)
                """,
                (
                    article_id,
                    "Understanding JSON Passthrough",
                    "JSON passthrough optimizes GraphQL performance...",
                    True,
                    author_id,
                    ["graphql", "performance", "json"],
                    json.dumps({"views": 1000, "likes": 50})
                )
            )

            await conn.commit()

        # Register types for development mode
        register_type_for_view("test_authors_view", Author)
        register_type_for_view("test_articles_view", Article)

        yield {"author_id": author_id, "article_id": article_id}

        # Cleanup
        async with db_pool.connection() as conn:
            await conn.execute("DROP VIEW IF EXISTS test_articles_view")
            await conn.execute("DROP VIEW IF EXISTS test_authors_view")
            await conn.execute("DROP TABLE IF EXISTS test_articles")
            await conn.execute("DROP TABLE IF EXISTS test_authors")

    @pytest.mark.asyncio
    async def test_development_mode_returns_typed_objects(self, db_pool, setup_test_schema):
        """Test that development mode returns fully typed Python objects."""
        # Create repository in development mode
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Fetch articles
        articles = await repo.find("test_articles_view")

        # Verify we get typed objects
        assert len(articles) == 1
        article = articles[0]

        # Check type instantiation
        assert isinstance(article, Article)
        assert isinstance(article.id, UUID)
        assert article.title == "Understanding JSON Passthrough"
        assert article.published is True
        assert article.tags == ["graphql", "performance", "json"]
        assert article.metadata == {"views": 1000, "likes": 50}

        # Check nested object instantiation
        assert isinstance(article.author, Author)
        assert article.author.name == "Jane Doe"
        assert article.author.email == "jane@example.com"

    @pytest.mark.asyncio
    async def test_production_mode_returns_dicts(self, db_pool, setup_test_schema):
        """Test that production mode returns plain dictionaries."""
        # Create repository in production mode
        repo = FraiseQLRepository(db_pool, context={"mode": "production"})

        # Fetch articles
        articles = await repo.find("test_articles_view")

        # Verify we get dicts
        assert len(articles) == 1
        article = articles[0]

        # Check it's a dict, not an Article instance
        assert isinstance(article, dict)
        assert not isinstance(article, Article)

        # Check data is still correct
        assert article["title"] == "Understanding JSON Passthrough"
        assert article["published"] is True
        assert article["tags"] == ["graphql", "performance", "json"]
        assert article["metadata"] == {"views": 1000, "likes": 50}

        # Check nested data is also a dict
        assert isinstance(article["data"]["author"], dict)
        assert article["data"]["author"]["name"] == "Jane Doe"

    @pytest.mark.asyncio
    async def test_json_passthrough_returns_raw_json(self, db_pool, setup_test_schema):
        """Test that JSON passthrough mode returns RawJSONResult."""
        # Create repository with JSON passthrough enabled
        repo = FraiseQLRepository(
            db_pool,
            context={
                "mode": "production",
                "json_passthrough": True,
                "graphql_info": None  # Simulate no GraphQL context
            }
        )

        # Use find_raw_json method with field name
        result = await repo.find_raw_json("test_articles_view", "articles")

        # Verify we get RawJSONResult
        assert isinstance(result, RawJSONResult)

        # Parse and verify the JSON content
        data = json.loads(result.json_string)
        assert "data" in data
        assert "articles" in data["data"]
        articles = data["data"]["articles"]
        assert len(articles) == 1
        assert articles[0]["title"] == "Understanding JSON Passthrough"

    @pytest.mark.asyncio
    async def test_performance_comparison(self, db_pool, setup_test_schema):
        """Compare performance between development and production modes."""
        import time

        # Insert more test data for meaningful comparison
        async with db_pool.connection() as conn:
            for i in range(100):
                author_id = uuid4()
                await conn.execute(
                    """
                    INSERT INTO test_authors (id, name, email, bio)
                    VALUES (%s, %s, %s, %s)
                    """,
                    (author_id, f"Author {i}", f"author{i}@example.com", f"Bio {i}")
                )

                for j in range(10):
                    await conn.execute(
                        """
                        INSERT INTO test_articles (id, title, content, published, author_id, tags, metadata)
                        VALUES (%s, %s, %s, %s, %s, %s, %s)
                        """,
                        (
                            uuid4(),
                            f"Article {i}-{j}",
                            f"Content for article {i}-{j}",
                            j % 2 == 0,
                            author_id,
                            [f"tag{k}" for k in range(5)],
                            json.dumps({"index": i * 10 + j})
                        )
                    )
            await conn.commit()

        # Measure development mode
        dev_repo = FraiseQLRepository(db_pool, context={"mode": "development"})
        start = time.time()
        dev_articles = await dev_repo.find("test_articles_view")
        dev_time = time.time() - start

        # Measure production mode
        prod_repo = FraiseQLRepository(db_pool, context={"mode": "production"})
        start = time.time()
        prod_articles = await prod_repo.find("test_articles_view")
        prod_time = time.time() - start

        # Measure JSON passthrough
        pass_repo = FraiseQLRepository(
            db_pool,
            context={"mode": "production", "json_passthrough": True}
        )
        start = time.time()
        pass_result = await pass_repo.find_raw_json("test_articles_view", "articles")
        pass_time = time.time() - start

        # Verify results
        assert len(dev_articles) == 1001  # Original + 1000 new
        assert len(prod_articles) == 1001
        pass_data = json.loads(pass_result.json_string)
        assert len(pass_data["data"]["articles"]) == 1001

        # Log performance (passthrough should be fastest)
        print(f"\nPerformance comparison:")
        print(f"Development mode: {dev_time:.3f}s")
        print(f"Production mode: {prod_time:.3f}s")
        print(f"JSON passthrough: {pass_time:.3f}s")

        # JSON passthrough should be faster than object instantiation
        assert pass_time < dev_time

    @pytest.mark.asyncio
    async def test_context_with_graphql_info_placeholder(self, db_pool, setup_test_schema):
        """Test that the repository works when GraphQL info is in context."""
        # Test that repository handles having graphql_info in context
        # Even if it's not a proper GraphQL info object

        repo = FraiseQLRepository(
            db_pool,
            context={
                "mode": "production",
                "json_passthrough": True,
                "graphql_info": None  # Placeholder - real GraphQL integration tested elsewhere
            }
        )

        # Should work normally
        articles = await repo.find("test_articles_view")
        assert len(articles) == 1
        assert "title" in articles[0]

    @pytest.mark.asyncio
    async def test_error_handling_in_different_modes(self, db_pool):
        """Test error handling in different execution modes."""
        # Test with non-existent view in production mode
        prod_repo = FraiseQLRepository(db_pool, context={"mode": "production"})
        with pytest.raises(Exception):  # Will raise a database error
            await prod_repo.find("non_existent_view")

    @pytest.mark.asyncio
    async def test_null_handling_across_modes(self, db_pool, setup_test_schema):
        """Test that null values are handled correctly in all modes."""
        # Insert author with null bio
        async with db_pool.connection() as conn:
            null_author_id = uuid4()
            await conn.execute(
                """
                INSERT INTO test_authors (id, name, email, bio)
                VALUES (%s, %s, %s, NULL)
                """,
                (null_author_id, "No Bio Author", "nobio@example.com")
            )
            await conn.commit()

        # Test development mode
        dev_repo = FraiseQLRepository(db_pool, context={"mode": "development"})
        authors = await dev_repo.find("test_authors_view")
        null_author = next(a for a in authors if a.name == "No Bio Author")
        assert null_author.bio is None

        # Test production mode
        prod_repo = FraiseQLRepository(db_pool, context={"mode": "production"})
        authors = await prod_repo.find("test_authors_view")
        null_author = next(a for a in authors if a["name"] == "No Bio Author")
        assert null_author["bio"] is None

        # Test JSON passthrough
        pass_repo = FraiseQLRepository(
            db_pool,
            context={"mode": "production", "json_passthrough": True}
        )
        result = await pass_repo.find_raw_json("test_authors_view", "authors")
        data = json.loads(result.json_string)
        null_author = next(a for a in data["data"]["authors"] if a["name"] == "No Bio Author")
        assert null_author["bio"] is None
