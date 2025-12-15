"""Tests for PostgreSQL full-text search filtering capabilities."""

import pytest
import pytest_asyncio

pytestmark = pytest.mark.database

# Import database fixtures for this database test
from tests.fixtures.database.database_conftest import *  # noqa: F403
from tests.unit.utils.test_response_utils import extract_graphql_data

import fraiseql
from fraiseql.db import FraiseQLRepository, register_type_for_view
from fraiseql.sql.where_generator import safe_create_where_type


class TestFullTextFilter:
    """Test PostgreSQL full-text search operators."""

    @pytest.fixture(scope="class")
    def test_types(self, clear_registry_class):
        """Create test types inside a fixture for proper isolation."""

        @fraiseql.type
        class Document:
            id: str
            title: str
            content: str
            search_vector: str  # This will be a tsvector column

        DocumentWhere = safe_create_where_type(Document)

        return {
            "Document": Document,
            "DocumentWhere": DocumentWhere,
        }

    @pytest_asyncio.fixture(scope="class")
    async def setup_test_documents(self, class_db_pool, test_schema, test_types):
        """Create test documents with tsvector data."""
        Document = test_types["Document"]
        # Register types for views (for development mode)
        register_type_for_view("test_documents_view", Document)

        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}, public")
            # Create tables
            await conn.execute(
                """
                CREATE TABLE IF NOT EXISTS test_documents (
                    id TEXT PRIMARY KEY,
                    title TEXT NOT NULL,
                    content TEXT NOT NULL,
                    search_vector TSVECTOR NOT NULL
                )
            """
            )

            # Create views with JSONB data column
            await conn.execute(
                """
                CREATE OR REPLACE VIEW test_documents_view AS
                SELECT
                    id, title, content, search_vector,
                    jsonb_build_object(
                        'id', id,
                        'title', title,
                        'content', content,
                        'search_vector', search_vector::text
                    ) as data
                FROM test_documents
            """
            )

            # Insert test data with tsvector values
            await conn.execute(
                """
                INSERT INTO test_documents (id, title, content, search_vector)
                VALUES
                    ('doc-001', 'Python Guide', 'Learn Python programming basics',
                     to_tsvector('english', 'Python Guide') || to_tsvector('english', 'Learn Python programming basics')),
                    ('doc-002', 'JavaScript Tutorial', 'Master JavaScript development',
                     to_tsvector('english', 'JavaScript Tutorial') || to_tsvector('english', 'Master JavaScript development')),
                    ('doc-003', 'Database Design', 'PostgreSQL best practices',
                     to_tsvector('english', 'Database Design') || to_tsvector('english', 'PostgreSQL best practices'))
            """
            )
            await conn.commit()

        yield

        # Cleanup happens automatically when test schema is dropped

    @pytest.mark.asyncio
    async def test_matches_operator_basic_search(
        self, class_db_pool, test_schema, setup_test_documents, test_types
    ) -> None:
        DocumentWhere = test_types["DocumentWhere"]
        """Test basic full-text search with matches operator."""
        repo = FraiseQLRepository(class_db_pool, context={"mode": "development"})

        # Test matches operator
        where = DocumentWhere(search_vector={"matches": "python"})

        result = await repo.find("test_documents_view", where=where)
        documents = extract_graphql_data(result, "test_documents_view")

        assert len(documents) == 1
        assert documents[0]["title"] == "Python Guide"

    @pytest.mark.asyncio
    async def test_plain_query_operator(
        self, class_db_pool, test_schema, setup_test_documents, test_types
    ) -> None:
        DocumentWhere = test_types["DocumentWhere"]
        """Test plain text query parsing."""
        repo = FraiseQLRepository(class_db_pool, context={"mode": "development"})

        # Test plain_query operator - should match documents with "javascript" AND "tutorial"
        where = DocumentWhere(search_vector={"plain_query": "javascript tutorial"})

        result = await repo.find("test_documents_view", where=where)
        documents = extract_graphql_data(result, "test_documents_view")

        assert len(documents) == 1
        assert documents[0]["title"] == "JavaScript Tutorial"

    @pytest.mark.asyncio
    async def test_phrase_query_operator(
        self, class_db_pool, test_schema, setup_test_documents, test_types
    ) -> None:
        DocumentWhere = test_types["DocumentWhere"]
        """Test phrase search query."""
        repo = FraiseQLRepository(class_db_pool, context={"mode": "development"})

        # Test phrase_query operator - should match exact phrase "programming basics"
        where = DocumentWhere(search_vector={"phrase_query": "programming basics"})

        result = await repo.find("test_documents_view", where=where)
        documents = extract_graphql_data(result, "test_documents_view")

        assert len(documents) == 1
        assert documents[0]["title"] == "Python Guide"

    @pytest.mark.asyncio
    async def test_websearch_query_operator(
        self, class_db_pool, test_schema, setup_test_documents, test_types
    ) -> None:
        DocumentWhere = test_types["DocumentWhere"]
        """Test websearch-style query parsing."""
        repo = FraiseQLRepository(class_db_pool, context={"mode": "development"})

        # Test websearch_query operator - supports AND/OR syntax like web search engines
        # Search for "javascript OR python"
        where = DocumentWhere(search_vector={"websearch_query": "javascript OR python"})

        result = await repo.find("test_documents_view", where=where)
        documents = extract_graphql_data(result, "test_documents_view")

        # Should match both JavaScript Tutorial and Python Guide
        assert len(documents) == 2
        titles = {doc["title"] for doc in documents}
        assert "JavaScript Tutorial" in titles
        assert "Python Guide" in titles

    @pytest.mark.asyncio
    async def test_rank_gt_operator(
        self, class_db_pool, test_schema, setup_test_documents, test_types
    ) -> None:
        DocumentWhere = test_types["DocumentWhere"]
        """Test relevance ranking greater than threshold."""
        repo = FraiseQLRepository(class_db_pool, context={"mode": "development"})

        # Test rank_gt operator - find documents with rank > 0.01 for "python"
        # Format is "query:threshold"
        where = DocumentWhere(search_vector={"rank_gt": "python:0.01"})

        result = await repo.find("test_documents_view", where=where)
        documents = extract_graphql_data(result, "test_documents_view")

        # Should match Python Guide (high relevance for "python")
        assert len(documents) >= 1
        assert any(doc["title"] == "Python Guide" for doc in documents)

    @pytest.mark.asyncio
    async def test_rank_lt_operator(
        self, class_db_pool, test_schema, setup_test_documents, test_types
    ) -> None:
        DocumentWhere = test_types["DocumentWhere"]
        """Test relevance ranking less than threshold."""
        repo = FraiseQLRepository(class_db_pool, context={"mode": "development"})

        # Test rank_lt operator - find documents with low rank for "python"
        where = DocumentWhere(search_vector={"rank_lt": "python:0.5"})

        result = await repo.find("test_documents_view", where=where)
        documents = extract_graphql_data(result, "test_documents_view")

        # Should match documents that don't have high relevance for "python"
        # (JavaScript Tutorial and Database Design)
        assert len(documents) >= 1
        titles = {doc["title"] for doc in documents}
        # At least one of these should be present (low relevance for "python")
        assert ("JavaScript Tutorial" in titles) or ("Database Design" in titles)

    @pytest.mark.asyncio
    async def test_rank_cd_gt_operator(
        self, class_db_pool, test_schema, setup_test_documents, test_types
    ) -> None:
        DocumentWhere = test_types["DocumentWhere"]
        """Test cover density ranking greater than threshold."""
        repo = FraiseQLRepository(class_db_pool, context={"mode": "development"})

        # Test rank_cd_gt operator - find documents with cover density rank > 0.01 for "javascript"
        where = DocumentWhere(search_vector={"rank_cd_gt": "javascript:0.01"})

        result = await repo.find("test_documents_view", where=where)
        documents = extract_graphql_data(result, "test_documents_view")

        # Should match JavaScript Tutorial (high cover density for "javascript")
        assert len(documents) >= 1
        assert any(doc["title"] == "JavaScript Tutorial" for doc in documents)

    @pytest.mark.asyncio
    async def test_rank_cd_lt_operator(
        self, class_db_pool, test_schema, setup_test_documents, test_types
    ) -> None:
        DocumentWhere = test_types["DocumentWhere"]
        """Test cover density ranking less than threshold."""
        repo = FraiseQLRepository(class_db_pool, context={"mode": "development"})

        # Test rank_cd_lt operator - find documents with low cover density for "javascript"
        where = DocumentWhere(search_vector={"rank_cd_lt": "javascript:0.5"})

        result = await repo.find("test_documents_view", where=where)
        documents = extract_graphql_data(result, "test_documents_view")

        # Should match documents with low cover density for "javascript"
        assert len(documents) >= 1
        titles = {doc["title"] for doc in documents}
        # At least one of these should be present
        assert ("Python Guide" in titles) or ("Database Design" in titles)
