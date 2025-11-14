# Embeddings Workflow Guide

**Complete guide to building RAG applications with FraiseQL, LangChain, and pgvector**

This guide covers the complete workflow from development to production deployment of embeddings-based applications (RAG, semantic search, recommendations).

## Table of Contents

1. [Quick Start](#quick-start)
2. [Architecture Overview](#architecture-overview)
3. [Development Workflow](#development-workflow)
4. [LangChain Integration](#langchain-integration)
5. [LlamaIndex Integration](#llamaindex-integration)
6. [Embedding Provider Selection](#embedding-provider-selection)
7. [Automation & CI/CD](#automation--cicd)
8. [Performance Optimization](#performance-optimization)
9. [Testing Strategies](#testing-strategies)
10. [Troubleshooting](#troubleshooting)

---

## Quick Start

### 1. Use the RAG Template

```bash
fraiseql init my-rag-app --template fastapi-rag
cd my-rag-app
```

### 2. Set Up Environment

```bash
# .env
DATABASE_URL=postgresql://user:pass@localhost/mydb
OPENAI_API_KEY=sk-...
```

### 3. Initialize Database

```bash
python scripts/setup_database.py
```

### 4. Run Application

```bash
uvicorn src.main:app --reload
```

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                   Your Application                           │
├─────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌─────────────────┐   │
│  │  LangChain   │  │  LlamaIndex  │  │   OpenAI API    │   │
│  │  (RAG logic) │  │  (Indexing)  │  │  (Embeddings)   │   │
│  └──────┬───────┘  └──────┬───────┘  └────────┬────────┘   │
│         │                 │                     │            │
│         └─────────────────┴─────────────────────┘            │
│                           ▼                                  │
│         ┌─────────────────────────────────────┐             │
│         │   FraiseQL Vector Store Integration  │             │
│         │   (src/fraiseql/integrations/)      │             │
│         └─────────────────┬───────────────────┘             │
│                           ▼                                  │
│         ┌─────────────────────────────────────┐             │
│         │      FraiseQL GraphQL API           │             │
│         │  (PostgreSQL → Rust → HTTP)         │             │
│         └─────────────────┬───────────────────┘             │
└───────────────────────────┼─────────────────────────────────┘
                            ▼
         ┌──────────────────────────────────────┐
         │       PostgreSQL + pgvector          │
         │  - Vector storage (vector type)      │
         │  - Distance operators (<->, <=>, etc)│
         │  - HNSW/IVFFlat indexes              │
         └──────────────────────────────────────┘
```

**Key Principle:** FraiseQL handles the **storage and retrieval layer**, while LangChain/LlamaIndex handle **embedding generation and RAG logic**.

---

## Development Workflow

### Step 1: Design Your Schema

```sql
-- migrations/001_create_documents.sql
CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE tb_document (
    pk_document SERIAL PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE tb_document_chunk (
    pk_chunk SERIAL PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT gen_random_uuid(),
    document_id UUID NOT NULL REFERENCES tb_document(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    embedding vector(1536),  -- OpenAI ada-002 dimension
    chunk_index INT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Create HNSW index for fast similarity search
CREATE INDEX idx_chunk_embedding ON tb_document_chunk
USING hnsw (embedding vector_cosine_ops)
WITH (m = 16, ef_construction = 64);

-- Create view for GraphQL
CREATE VIEW v_document AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'title', title,
        'content', content,
        'created_at', created_at
    ) as data
FROM tb_document;

CREATE VIEW v_document_chunk AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'document_id', document_id,
        'content', content,
        'embedding', embedding,
        'chunk_index', chunk_index
    ) as data
FROM tb_document_chunk;
```

### Step 2: Define FraiseQL Types

```python
# src/models.py
from typing import List
from datetime import datetime
from fraiseql import type, fraise_field
from fraiseql.types.scalars import UUID

@type(sql_source="v_document", jsonb_column="data")
class Document:
    """Document with content."""
    id: UUID
    title: str
    content: str
    created_at: datetime

@type(sql_source="v_document_chunk", jsonb_column="data")
class DocumentChunk:
    """Document chunk with embedding."""
    id: UUID
    document_id: UUID
    content: str
    embedding: List[float]  # Automatically detected as vector type
    chunk_index: int
```

### Step 3: Set Up Vector Store

```python
# src/vector_store.py
import os
from langchain_openai import OpenAIEmbeddings
from langchain_community.vectorstores import PGVector

def create_vector_store():
    """Initialize pgvector store with LangChain."""
    embeddings = OpenAIEmbeddings(
        model="text-embedding-ada-002"  # or "text-embedding-3-small"
    )

    connection_string = os.getenv("DATABASE_URL")

    vector_store = PGVector(
        collection_name="document_chunks",
        connection_string=connection_string,
        embedding_function=embeddings,
    )

    return vector_store
```

### Step 4: Implement GraphQL Queries

```python
# src/queries.py
from typing import List
from fraiseql import query
from .models import Document
from .vector_store import create_vector_store

@query
async def search_documents(
    info,
    query: str,
    limit: int = 10
) -> List[Document]:
    """Semantic search across documents."""
    # Get vector store from context
    vector_store = info.context["vector_store"]

    # Perform similarity search
    results = vector_store.similarity_search_with_score(
        query,
        k=limit
    )

    # Convert to Document objects
    documents = []
    for doc, score in results:
        doc_id = doc.metadata.get("document_id")
        if doc_id:
            # Fetch full document from database
            repo = info.context["db"]
            doc_data = await repo.find_one(
                "v_document",
                where={"id": doc_id}
            )
            if doc_data:
                documents.append(Document(**doc_data))

    return documents
```

### Step 5: Implement Mutations

```python
# src/mutations.py
from langchain.text_splitter import RecursiveCharacterTextSplitter
from langchain_core.documents import Document as LangChainDoc

@mutation
async def upload_document(
    info,
    title: str,
    content: str
) -> Document:
    """Upload document and generate embeddings."""
    repo = info.context["db"]
    vector_store = info.context["vector_store"]

    # Create document in database
    result = await repo.call_function(
        "fn_create_document",
        p_title=title,
        p_content=content
    )
    doc_id = result["id"]

    # Split into chunks
    text_splitter = RecursiveCharacterTextSplitter(
        chunk_size=1000,
        chunk_overlap=200
    )
    chunks = text_splitter.split_text(content)

    # Generate embeddings and store
    langchain_docs = []
    for i, chunk in enumerate(chunks):
        # Store chunk in database
        await repo.call_function(
            "fn_create_document_chunk",
            p_document_id=doc_id,
            p_content=chunk,
            p_chunk_index=i
        )

        # Prepare for vector store
        langchain_docs.append(
            LangChainDoc(
                page_content=chunk,
                metadata={
                    "document_id": str(doc_id),
                    "chunk_index": i
                }
            )
        )

    # Add to vector store (generates embeddings automatically)
    if langchain_docs:
        vector_store.add_documents(langchain_docs)

    # Return created document
    doc_data = await repo.find_one("v_document", where={"id": doc_id})
    return Document(**doc_data)
```

---

## LangChain Integration

FraiseQL provides `FraiseQLVectorStore` for LangChain integration.

### Basic Usage

```python
from fraiseql.integrations.langchain import FraiseQLVectorStore
from langchain_openai import OpenAIEmbeddings

# Initialize
vector_store = FraiseQLVectorStore(
    collection_name="documents",
    connection_string=DATABASE_URL,
    embedding_function=OpenAIEmbeddings(),
    # Optional configuration
    distance_metric="cosine",  # cosine, l2, inner_product
    pre_delete_collection=False  # Don't delete existing data
)

# Add documents
await vector_store.aadd_texts(
    texts=["Document 1 content", "Document 2 content"],
    metadatas=[{"source": "doc1"}, {"source": "doc2"}]
)

# Search
results = await vector_store.asimilarity_search(
    query="search query",
    k=5
)

# Search with scores
results_with_scores = await vector_store.asimilarity_search_with_score(
    query="search query",
    k=5
)

# Delete
await vector_store.adelete(ids=["doc-id-1", "doc-id-2"])
```

### Advanced: Custom Metadata Filtering

```python
# Add documents with rich metadata
await vector_store.aadd_texts(
    texts=chunks,
    metadatas=[
        {
            "document_id": doc_id,
            "chunk_index": i,
            "category": "technical",
            "created_at": datetime.now().isoformat()
        }
        for i in range(len(chunks))
    ]
)

# Filter during search
results = await vector_store.asimilarity_search(
    query="machine learning",
    k=10,
    filter={"category": "technical"}  # PostgreSQL JSONB filtering
)
```

### RAG Chain Example

```python
from langchain.chains import RetrievalQA
from langchain_openai import ChatOpenAI

# Create retriever
retriever = vector_store.as_retriever(
    search_type="similarity",
    search_kwargs={"k": 5}
)

# Create RAG chain
qa_chain = RetrievalQA.from_chain_type(
    llm=ChatOpenAI(model="gpt-4"),
    chain_type="stuff",
    retriever=retriever,
    return_source_documents=True
)

# Ask questions
result = qa_chain({"query": "What is FraiseQL?"})
print(result["result"])
for doc in result["source_documents"]:
    print(f"Source: {doc.metadata}")
```

---

## LlamaIndex Integration

FraiseQL also integrates with LlamaIndex for advanced RAG workflows.

### Basic Usage

```python
from fraiseql.integrations.llamaindex import FraiseQLVectorStore
from llama_index.core import VectorStoreIndex, Document
from llama_index.embeddings.openai import OpenAIEmbedding

# Initialize
embed_model = OpenAIEmbedding()
vector_store = FraiseQLVectorStore(
    connection_string=DATABASE_URL,
    table_name="embeddings",
    embed_dim=1536
)

# Create index
index = VectorStoreIndex.from_vector_store(
    vector_store=vector_store,
    embed_model=embed_model
)

# Add documents
documents = [
    Document(text="Document 1 content"),
    Document(text="Document 2 content")
]
for doc in documents:
    index.insert(doc)

# Query
query_engine = index.as_query_engine()
response = query_engine.query("What is this about?")
print(response)
```

---

## Embedding Provider Selection

Choose the right embedding provider for your use case:

### OpenAI (Cloud, Paid)

**Pros:**
- High quality embeddings
- No infrastructure management
- Fast API

**Cons:**
- Recurring costs (~$0.10 per 1M tokens)
- Data sent to third party
- Rate limits

```python
from langchain_openai import OpenAIEmbeddings

embeddings = OpenAIEmbeddings(
    model="text-embedding-3-small",  # 1536 dims, $0.02/1M tokens
    # or "text-embedding-3-large"   # 3072 dims, $0.13/1M tokens
)
```

### Sentence Transformers (Local, Free)

**Pros:**
- No API costs
- Full data privacy
- No rate limits

**Cons:**
- Requires GPU for good performance
- Infrastructure overhead
- Slightly lower quality than OpenAI

```python
from langchain_community.embeddings import HuggingFaceEmbeddings

embeddings = HuggingFaceEmbeddings(
    model_name="sentence-transformers/all-MiniLM-L6-v2",  # 384 dims
    # or "sentence-transformers/all-mpnet-base-v2"       # 768 dims
    model_kwargs={"device": "cuda"}  # Use GPU
)
```

### Anthropic (Cloud, Paid)

**Pros:**
- High quality
- Privacy-focused company

**Cons:**
- Currently in beta
- Similar costs to OpenAI

```python
from langchain_anthropic import AnthropicEmbeddings

embeddings = AnthropicEmbeddings()
```

### Comparison Table

| Provider | Dimensions | Cost/1M tokens | Quality | Privacy | Speed |
|----------|-----------|----------------|---------|---------|-------|
| OpenAI ada-002 | 1536 | $0.10 | ⭐⭐⭐⭐⭐ | ❌ Cloud | ⚡⚡⚡ |
| OpenAI 3-small | 1536 | $0.02 | ⭐⭐⭐⭐ | ❌ Cloud | ⚡⚡⚡ |
| OpenAI 3-large | 3072 | $0.13 | ⭐⭐⭐⭐⭐ | ❌ Cloud | ⚡⚡⚡ |
| all-MiniLM-L6-v2 | 384 | Free | ⭐⭐⭐ | ✅ Local | ⚡⚡ |
| all-mpnet-base-v2 | 768 | Free | ⭐⭐⭐⭐ | ✅ Local | ⚡ |

---

## Automation & CI/CD

### Regenerate Embeddings Script

```python
# scripts/regenerate_embeddings.py
"""Regenerate embeddings for an entity."""
import asyncio
import os
from langchain_openai import OpenAIEmbeddings
from langchain_community.vectorstores import PGVector
from langchain.text_splitter import RecursiveCharacterTextSplitter
from fraiseql.db import create_repository

async def regenerate_embeddings(entity: str):
    """Regenerate all embeddings for an entity."""
    print(f"Regenerating embeddings for {entity}...")

    # Initialize components
    db_url = os.getenv("DATABASE_URL")
    repo = create_repository(db_url)
    embeddings = OpenAIEmbeddings()

    vector_store = PGVector(
        collection_name=entity.lower(),
        connection_string=db_url,
        embedding_function=embeddings
    )

    # Fetch all documents
    documents = await repo.find(f"v_{entity.lower()}")

    # Clear existing embeddings
    print(f"Clearing existing embeddings...")
    await vector_store.adelete_collection()

    # Regenerate
    text_splitter = RecursiveCharacterTextSplitter(
        chunk_size=1000,
        chunk_overlap=200
    )

    total_chunks = 0
    for doc in documents:
        chunks = text_splitter.split_text(doc["content"])
        await vector_store.aadd_texts(
            texts=chunks,
            metadatas=[
                {"document_id": doc["id"], "chunk_index": i}
                for i in range(len(chunks))
            ]
        )
        total_chunks += len(chunks)

    print(f"✓ Regenerated {total_chunks} chunks for {len(documents)} documents")

if __name__ == "__main__":
    import sys
    entity = sys.argv[1] if len(sys.argv) > 1 else "Document"
    asyncio.run(regenerate_embeddings(entity))
```

Usage:

```bash
python scripts/regenerate_embeddings.py Document
```

### CI/CD Pipeline Example

```yaml
# .github/workflows/embeddings.yml
name: Regenerate Embeddings

on:
  schedule:
    - cron: '0 2 * * 0'  # Weekly on Sunday at 2 AM
  workflow_dispatch:  # Manual trigger

jobs:
  regenerate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Set up Python
        uses: actions/setup-python@v4
        with:
          python-version: '3.13'

      - name: Install dependencies
        run: |
          pip install fraiseql langchain langchain-openai

      - name: Regenerate embeddings
        env:
          DATABASE_URL: ${{ secrets.DATABASE_URL }}
          OPENAI_API_KEY: ${{ secrets.OPENAI_API_KEY }}
        run: |
          python scripts/regenerate_embeddings.py Document

      - name: Validate embeddings
        run: |
          fraiseql vector validate tb_document_chunk embedding
```

### Batch Processing Script

```python
# scripts/batch_process_embeddings.py
"""Process embeddings in batches to avoid rate limits."""
import asyncio
from typing import List
import time

async def batch_process_embeddings(
    documents: List[dict],
    batch_size: int = 50,
    delay: float = 1.0
):
    """Process embeddings in batches with rate limiting."""
    for i in range(0, len(documents), batch_size):
        batch = documents[i:i + batch_size]

        print(f"Processing batch {i//batch_size + 1}/{len(documents)//batch_size + 1}...")

        # Process batch
        await vector_store.aadd_texts(
            texts=[doc["content"] for doc in batch],
            metadatas=[{"id": doc["id"]} for doc in batch]
        )

        # Rate limiting
        if i + batch_size < len(documents):
            time.sleep(delay)

    print("✓ Batch processing complete")
```

---

## Performance Optimization

### 1. Choose the Right Index

```sql
-- HNSW: Best for most use cases (fast queries, good recall)
CREATE INDEX idx_embedding_hnsw ON tb_chunks
USING hnsw (embedding vector_cosine_ops)
WITH (m = 16, ef_construction = 64);

-- IVFFlat: Better for very large datasets (>1M vectors)
CREATE INDEX idx_embedding_ivfflat ON tb_chunks
USING ivfflat (embedding vector_cosine_ops)
WITH (lists = 100);
```

### 2. Optimize Vector Dimensions

**Use HalfVec for 50% memory savings:**

```python
from fraiseql.types.scalars import HalfVectorScalar

@type
class DocumentChunk:
    embedding: List[float]  # Will use halfvec if field name matches pattern
```

```sql
-- Use halfvec type
CREATE TABLE tb_chunks (
    id UUID PRIMARY KEY,
    embedding halfvec(1536)  -- 50% smaller than vector(1536)
);
```

### 3. Monitor Query Performance

```sql
-- Check index usage
EXPLAIN ANALYZE
SELECT id, embedding <=> '[0.1, 0.2, ...]'::vector as distance
FROM tb_chunks
ORDER BY embedding <=> '[0.1, 0.2, ...]'::vector
LIMIT 10;

-- Should see "Index Scan using idx_embedding_hnsw"
```

### 4. Batch Operations

```python
# Bad: One at a time (slow)
for text in texts:
    await vector_store.aadd_texts([text])

# Good: Batch insert (fast)
await vector_store.aadd_texts(texts)
```

---

## Testing Strategies

### 1. Use Fake Embeddings in Tests

```python
from langchain_community.embeddings import FakeEmbeddings

@pytest.fixture
async def test_vector_store(db_url):
    """Test vector store with fake embeddings (no API calls)."""
    return PGVector(
        collection_name="test_docs",
        connection_string=db_url,
        embedding_function=FakeEmbeddings(size=384)
    )

async def test_similarity_search(test_vector_store):
    """Test similarity search without real API calls."""
    # Add test data
    await test_vector_store.aadd_texts(
        texts=["Python programming", "JavaScript tutorial"],
        metadatas=[{"lang": "python"}, {"lang": "js"}]
    )

    # Search
    results = await test_vector_store.asimilarity_search("coding", k=2)
    assert len(results) == 2
```

### 2. Dimension Validation

```python
async def test_embedding_dimensions(repo):
    """Validate all embeddings have correct dimensions."""
    result = await repo.execute("""
        SELECT COUNT(*) as invalid_count
        FROM tb_chunks
        WHERE array_length(embedding, 1) != 1536
    """)

    assert result[0]["invalid_count"] == 0, "Found embeddings with wrong dimensions"
```

### 3. Index Coverage

```python
async def test_vector_indexes_exist(repo):
    """Ensure vector indexes are created."""
    result = await repo.execute("""
        SELECT indexname
        FROM pg_indexes
        WHERE tablename = 'tb_chunks'
          AND indexname LIKE '%embedding%'
    """)

    assert len(result) > 0, "No vector indexes found"
```

---

## Troubleshooting

### Issue: "extension vector does not exist"

```sql
CREATE EXTENSION vector;
```

### Issue: Slow queries

```sql
-- Check if index is being used
EXPLAIN ANALYZE
SELECT * FROM tb_chunks
ORDER BY embedding <=> '[...]'::vector
LIMIT 10;

-- If not using index, create one
CREATE INDEX idx_embedding ON tb_chunks
USING hnsw (embedding vector_cosine_ops);
```

### Issue: Dimension mismatch

```python
# Check actual dimensions
result = await repo.execute("""
    SELECT DISTINCT array_length(embedding, 1) as dims
    FROM tb_chunks
""")
print(f"Found dimensions: {result}")

# Ensure embedding model matches
embeddings = OpenAIEmbeddings(model="text-embedding-ada-002")  # 1536 dims
```

### Issue: Rate limits with OpenAI

```python
# Add retry logic
from tenacity import retry, stop_after_attempt, wait_exponential

@retry(
    stop=stop_after_attempt(3),
    wait=wait_exponential(multiplier=1, min=4, max=10)
)
async def add_with_retry(vector_store, texts):
    return await vector_store.aadd_texts(texts)
```

### Issue: Out of memory

```python
# Process in smaller batches
batch_size = 50
for i in range(0, len(texts), batch_size):
    batch = texts[i:i + batch_size]
    await vector_store.aadd_texts(batch)
    await asyncio.sleep(1)  # Rate limiting
```

---

## Summary

**FraiseQL's Role:**
- ✅ Efficient vector storage (pgvector)
- ✅ Fast GraphQL queries with distance operators
- ✅ Integration layer for LangChain/LlamaIndex

**External Tools' Role:**
- 🔧 LangChain/LlamaIndex: RAG logic and orchestration
- 🤖 OpenAI/HuggingFace: Embedding generation

**Best Practices:**
1. Use FraiseQL for the database/API layer
2. Use LangChain/LlamaIndex for RAG logic
3. Choose embedding provider based on cost/quality/privacy needs
4. Create automation scripts for batch operations
5. Use fake embeddings in tests
6. Monitor index usage and query performance

For more examples, see:
- `templates/fastapi-rag/` - Complete RAG application
- `src/fraiseql/integrations/langchain.py` - LangChain integration
- `src/fraiseql/integrations/llamaindex.py` - LlamaIndex integration

**Questions?** Check the [Troubleshooting](#troubleshooting) section or open an issue on GitHub.
