"""Integration tests for FraiseQL LangChain VectorStore.

Tests complete LangChain VectorStore functionality including:
- Document storage and retrieval
- Vector similarity search
- Metadata filtering
- Error handling and edge cases
- Performance with real database operations
"""

import uuid
from typing import List
from unittest.mock import Mock

import pytest
import pytest_asyncio

# Check if LangChain is available
try:
    from langchain_core.documents import Document  # type: ignore

    LANGCHAIN_AVAILABLE = True
except ImportError:
    LANGCHAIN_AVAILABLE = False
    Document = Mock  # type: ignore

from fraiseql.integrations.langchain import FraiseQLVectorStore

pytestmark = pytest.mark.integration


# Mock embedding function for testing
class MockEmbeddings:
    """Mock embeddings class for testing."""

    def __init__(self, dimension: int = 384):
        self.dimension = dimension

    async def aembed_documents(self, texts: List[str]) -> List[List[float]]:
        """Mock async embed documents."""
        return [[0.1] * self.dimension for _ in texts]

    async def aembed_query(self, text: str) -> List[float]:
        """Mock async embed query."""
        return [0.1] * self.dimension

    def embed_query(self, text: str) -> List[float]:
        """Mock sync embed query."""
        return [0.1] * self.dimension


@pytest_asyncio.fixture
async def vectorstore_table(class_db_pool, test_schema, pgvector_available):
    """Create a test table for vectorstore integration tests."""
    if not pgvector_available:
        pytest.skip("pgvector extension not available")

    table_name = f"test_langchain_docs_{uuid.uuid4().hex[:8]}"

    async with class_db_pool.connection() as conn:
        await conn.execute(f"SET search_path TO {test_schema}, public")

        # Create test table with vector support
        await conn.execute(f"""
            CREATE TABLE {table_name} (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                embedding vector(384),
                metadata JSONB
            );
        """)

        # Create vector index for performance
        try:
            await conn.execute(f"""
                CREATE INDEX {table_name}_embedding_idx
                ON {table_name} USING ivfflat (embedding vector_cosine_ops)
                WITH (lists = 100);
            """)
        except Exception:
            # Index creation might fail in some test environments, continue anyway
            pass

    yield table_name


@pytest.fixture
def mock_embeddings() -> MockEmbeddings:
    """Mock embeddings for testing."""
    return MockEmbeddings(dimension=384)


@pytest_asyncio.fixture
async def vectorstore(
    class_db_pool, vectorstore_table: str, mock_embeddings: MockEmbeddings, pgvector_available
) -> FraiseQLVectorStore:
    """Create a FraiseQLVectorStore instance for testing."""
    if not pgvector_available:
        pytest.skip("pgvector extension not available")

    return FraiseQLVectorStore(
        db_pool=class_db_pool,
        table_name=vectorstore_table,
        embedding_function=mock_embeddings,
        embedding_column="embedding",
        content_column="content",
        metadata_column="metadata",
        id_column="id",
        distance_metric="cosine",
    )


class TestLangChainVectorStoreIntegration:
    """Integration tests for LangChain VectorStore functionality."""

    @pytest.mark.skipif(not LANGCHAIN_AVAILABLE, reason="LangChain not available")
    @pytest.mark.skipif(not LANGCHAIN_AVAILABLE, reason="LangChain not available")
    @pytest.mark.asyncio
    async def test_add_documents_basic(self, vectorstore: FraiseQLVectorStore) -> None:
        """Test basic document addition functionality."""
        documents = [
            Document(
                page_content="This is a test document about AI.",
                metadata={"category": "tech", "author": "John"},
            ),
            Document(
                page_content="Another document about machine learning.",
                metadata={"category": "tech", "author": "Jane"},
            ),
            Document(
                page_content="A document about cooking recipes.",
                metadata={"category": "food", "author": "Bob"},
            ),
        ]

        # Add documents
        ids = await vectorstore.aadd_documents(documents)

        # Verify IDs were returned
        assert len(ids) == 3
        assert all(isinstance(id, str) for id in ids)
        assert len(set(ids)) == 3  # All IDs unique

    @pytest.mark.skipif(not LANGCHAIN_AVAILABLE, reason="LangChain not available")
    @pytest.mark.skipif(not LANGCHAIN_AVAILABLE, reason="LangChain not available")
    @pytest.mark.asyncio
    async def test_add_texts_with_metadata(self, vectorstore: FraiseQLVectorStore) -> None:
        """Test adding texts with metadata."""
        texts = ["First document content", "Second document content", "Third document content"]

        metadatas = [
            {"source": "web", "topic": "AI"},
            {"source": "book", "topic": "ML"},
            {"source": "article", "topic": "Data Science"},
        ]

        # Add texts with metadata
        ids = await vectorstore.aadd_texts(texts, metadatas)

        # Verify
        assert len(ids) == 3

        # Query back to verify data was stored
        results = await vectorstore.asimilarity_search("document", k=10)
        assert len(results) == 3

        # Check metadata was stored correctly
        for result in results:
            assert "source" in result.metadata
            assert "topic" in result.metadata

    @pytest.mark.skipif(not LANGCHAIN_AVAILABLE, reason="LangChain not available")
    @pytest.mark.asyncio
    async def test_similarity_search_basic(self, vectorstore: FraiseQLVectorStore) -> None:
        """Test basic similarity search functionality."""
        # Add test documents
        texts = [
            "The quick brown fox jumps over the lazy dog",
            "A story about artificial intelligence and machine learning",
            "Cooking recipes for delicious pasta dishes",
            "The history of computer science and programming",
        ]

        metadatas = [
            {"category": "animals"},
            {"category": "tech"},
            {"category": "food"},
            {"category": "tech"},
        ]

        await vectorstore.aadd_texts(texts, metadatas)

        # Perform similarity search
        results = await vectorstore.asimilarity_search("artificial intelligence", k=2)

        # Verify results
        assert len(results) == 2
        assert all(isinstance(result, object) for result in results)  # Should be Document objects

        # Results should be relevant (tech category documents)
        result_categories = [doc.metadata.get("category") for doc in results]
        assert "tech" in result_categories

    @pytest.mark.skipif(not LANGCHAIN_AVAILABLE, reason="LangChain not available")
    @pytest.mark.asyncio
    async def test_similarity_search_with_filter(self, vectorstore: FraiseQLVectorStore) -> None:
        """Test similarity search with metadata filtering."""
        # Add test documents
        texts = [
            "AI and machine learning guide",
            "Cooking pasta recipes",
            "AI research paper",
            "Italian cooking techniques",
        ]

        metadatas = [
            {"category": "tech", "author": "Alice"},
            {"category": "food", "author": "Bob"},
            {"category": "tech", "author": "Charlie"},
            {"category": "food", "author": "David"},
        ]

        await vectorstore.aadd_texts(texts, metadatas)

        # Search with filter for tech category only
        results = await vectorstore.asimilarity_search(
            "artificial intelligence", k=10, filter={"category": "tech"}
        )

        # Verify all results have tech category
        assert len(results) >= 1
        for result in results:
            assert result.metadata.get("category") == "tech"

    @pytest.mark.skipif(not LANGCHAIN_AVAILABLE, reason="LangChain not available")
    @pytest.mark.asyncio
    async def test_similarity_search_multiple_filters(
        self, vectorstore: FraiseQLVectorStore
    ) -> None:
        """Test similarity search with multiple metadata filters."""
        # Add test documents
        texts = [
            "AI programming tutorial",
            "Pasta cooking class",
            "Machine learning research",
            "Recipe for carbonara",
            "Deep learning guide",
        ]

        metadatas = [
            {"category": "tech", "author": "Alice", "difficulty": "beginner"},
            {"category": "food", "author": "Bob", "difficulty": "intermediate"},
            {"category": "tech", "author": "Alice", "difficulty": "advanced"},
            {"category": "food", "author": "Charlie", "difficulty": "intermediate"},
            {"category": "tech", "author": "Bob", "difficulty": "advanced"},
        ]

        await vectorstore.aadd_texts(texts, metadatas)

        # Search with multiple filters
        results = await vectorstore.asimilarity_search(
            "tutorial", k=10, filter={"category": "tech", "author": "Alice"}
        )

        # Verify all results match both filters
        assert len(results) >= 1
        for result in results:
            assert result.metadata.get("category") == "tech"
            assert result.metadata.get("author") == "Alice"

    @pytest.mark.skipif(not LANGCHAIN_AVAILABLE, reason="LangChain not available")
    @pytest.mark.asyncio
    async def test_similarity_search_with_score(self, vectorstore: FraiseQLVectorStore) -> None:
        """Test similarity search with scores (placeholder implementation)."""
        # Add test documents
        texts = ["Document one", "Document two", "Document three"]
        await vectorstore.aadd_texts(texts)

        # Search with scores
        results = await vectorstore.asimilarity_search_with_score("document", k=2)

        # Verify structure (scores are currently placeholder 0.0)
        assert len(results) == 2
        for doc, score in results:
            assert hasattr(doc, "page_content")
            assert hasattr(doc, "metadata")
            assert isinstance(score, float)

    @pytest.mark.skipif(not LANGCHAIN_AVAILABLE, reason="LangChain not available")
    @pytest.mark.asyncio
    async def test_delete_documents(self, vectorstore: FraiseQLVectorStore) -> None:
        """Test document deletion functionality."""
        # Add documents
        texts = ["Doc 1", "Doc 2", "Doc 3"]
        ids = await vectorstore.aadd_texts(texts)

        # Verify documents exist
        results = await vectorstore.asimilarity_search("Doc", k=10)
        assert len(results) == 3

        # Delete one document
        await vectorstore.adelete([ids[0]])

        # Verify document was deleted
        results = await vectorstore.asimilarity_search("Doc", k=10)
        assert len(results) == 2

        # Verify deleted document is not in results
        remaining_ids = [doc.metadata.get("id") for doc in results]
        assert ids[0] not in remaining_ids

    @pytest.mark.skipif(not LANGCHAIN_AVAILABLE, reason="LangChain not available")
    @pytest.mark.asyncio
    async def test_empty_search_results(self, vectorstore: FraiseQLVectorStore) -> None:
        """Test behavior with empty search results."""
        # Search without adding any documents
        results = await vectorstore.asimilarity_search("nonexistent content", k=5)
        assert results == []

    @pytest.mark.skipif(not LANGCHAIN_AVAILABLE, reason="LangChain not available")
    @pytest.mark.asyncio
    async def test_large_k_parameter(self, vectorstore: FraiseQLVectorStore) -> None:
        """Test behavior with large k parameter."""
        # Add a few documents
        texts = ["Doc 1", "Doc 2", "Doc 3"]
        await vectorstore.aadd_texts(texts)

        # Search with k larger than available documents
        results = await vectorstore.asimilarity_search("Doc", k=10)
        assert len(results) == 3  # Should return all available documents

    @pytest.mark.skipif(not LANGCHAIN_AVAILABLE, reason="LangChain not available")
    @pytest.mark.asyncio
    async def test_filter_no_matches(self, vectorstore: FraiseQLVectorStore) -> None:
        """Test filtering with no matching documents."""
        # Add documents
        texts = ["AI content", "Food content"]
        metadatas = [{"category": "tech"}, {"category": "food"}]
        await vectorstore.aadd_texts(texts, metadatas)

        # Search with filter that matches no documents
        results = await vectorstore.asimilarity_search(
            "content", k=5, filter={"category": "nonexistent"}
        )
        assert results == []

    @pytest.mark.skipif(not LANGCHAIN_AVAILABLE, reason="LangChain not available")
    def test_synchronous_methods(self, vectorstore: FraiseQLVectorStore) -> None:
        """Test synchronous wrapper methods."""
        from langchain_core.documents import Document

        # Test sync add_documents
        documents = [Document(page_content="Sync test document", metadata={"test": "sync"})]
        ids = vectorstore.add_documents(documents)
        assert len(ids) == 1

        # Test sync similarity_search
        results = vectorstore.similarity_search("document", k=5)
        assert len(results) >= 1

        # Test sync similarity_search_with_score
        results_with_scores = vectorstore.similarity_search_with_score("document", k=5)
        assert len(results_with_scores) >= 1

    @pytest.mark.skipif(not LANGCHAIN_AVAILABLE, reason="LangChain not available")
    @pytest.mark.asyncio
    async def test_embeddings_property(self, vectorstore: FraiseQLVectorStore) -> None:
        """Test embeddings property access."""
        embeddings = vectorstore.embeddings
        assert embeddings is not None
        assert hasattr(embeddings, "aembed_query")

    @pytest.mark.skipif(not LANGCHAIN_AVAILABLE, reason="LangChain not available")
    @pytest.mark.asyncio
    async def test_different_distance_metrics(
        self, class_db_pool, test_schema, vectorstore_table: str, mock_embeddings: MockEmbeddings
    ) -> None:
        """Test different distance metrics."""
        # Test L2 distance
        vectorstore_l2 = FraiseQLVectorStore(
            db_pool=class_db_pool,
            table_name=vectorstore_table,
            embedding_function=mock_embeddings,
            distance_metric="l2",
        )

        texts = ["Test document"]
        await vectorstore_l2.aadd_texts(texts)

        results = await vectorstore_l2.asimilarity_search("test", k=1)
        assert len(results) == 1

        # Test inner product distance
        vectorstore_ip = FraiseQLVectorStore(
            db_pool=class_db_pool,
            table_name=vectorstore_table,
            embedding_function=mock_embeddings,
            distance_metric="inner_product",
        )

        results = await vectorstore_ip.asimilarity_search("test", k=1)
        assert len(results) == 1
