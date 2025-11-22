# Vector Search Example

This example demonstrates FraiseQL's vector similarity search capabilities using PostgreSQL's pgvector extension. It showcases all six distance operators including the new binary vector operators (Hamming and Jaccard) for advanced similarity search use cases.

## Features Demonstrated

- **Semantic Search**: Cosine distance for text embeddings
- **Spatial Similarity**: L2 distance for geometric data
- **Learned Metrics**: Inner product for trained embeddings
- **Sparse Vectors**: L1 distance for Manhattan distance
- **Binary Vectors**: Hamming and Jaccard distances for hash-based similarity
- **Hybrid Search**: Combining vector similarity with metadata filters
- **Ordering**: Results ordered by vector distance

## Setup

### 1. Install pgvector

```sql
-- Enable the pgvector extension
CREATE EXTENSION vector;
```

### 2. Create Schema

```sql
-- Create documents table with vector embeddings
CREATE TABLE documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    embedding vector(384),  -- Text embedding (e.g., all-MiniLM-L6-v2)
    binary_hash bit(64),   -- Binary hash for Hamming/Jaccard
    category TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create indexes for performance
CREATE INDEX ON documents USING hnsw (embedding vector_cosine_ops);
CREATE INDEX ON documents (category);
```

### 3. Install Dependencies

```bash
pip install fraiseql pgvector numpy
```

## Usage Examples

### Semantic Search with Cosine Distance

```python
from fraiseql import FraiseQLRepository
import asyncio

async def semantic_search():
    repo = FraiseQLRepository("postgresql://localhost/vectordb")

    # Search for documents similar to a query embedding
    query_embedding = [0.1, 0.2, 0.3] * 128  # 384 dimensions

    results = await repo.find(
        "documents",
        where={"embedding": {"cosine_distance": query_embedding}},
        orderBy={"embedding": {"cosine_distance": query_embedding}},
        limit=10
    )

    for doc in results:
        print(f"{doc['title']}: {doc['cosine_distance']:.3f}")

asyncio.run(semantic_search())
```

### Binary Vector Search with Hamming Distance

```python
async def binary_search():
    repo = FraiseQLRepository("postgresql://localhost/vectordb")

    # Search using binary hash similarity
    query_hash = "1010101010101010"  # 16-bit binary string

    results = await repo.find(
        "documents",
        where={"binary_hash": {"hamming_distance": query_hash}},
        orderBy={"binary_hash": {"hamming_distance": query_hash}},
        limit=10
    )

    for doc in results:
        print(f"{doc['title']}: hamming_distance={doc['hamming_distance']}")

asyncio.run(binary_search())
```

### Hybrid Search (Vector + Metadata)

```python
async def hybrid_search():
    repo = FraiseQLRepository("postgresql://localhost/vectordb")

    query_embedding = [0.1, 0.2, 0.3] * 128

    results = await repo.find(
        "documents",
        where={
            "embedding": {"cosine_distance": query_embedding},
            "category": {"eq": "technical"},
            "created_at": {"gte": "2024-01-01"}
        },
        orderBy={"embedding": {"cosine_distance": query_embedding}},
        limit=20
    )

    return results

asyncio.run(hybrid_search())
```

## GraphQL API

### Vector Filtering

```graphql
query SearchDocuments(
  $embedding: [Float!]!
  $binaryHash: String
  $category: String
  $limit: Int
) {
  documents(
    where: {
      embedding: { cosine_distance: $embedding }
      binary_hash: { hamming_distance: $binaryHash }
      category: { eq: $category }
    }
    orderBy: {
      embedding: { cosine_distance: $embedding }
    }
    limit: $limit
  ) {
    id
    title
    content
    category
    cosine_distance
    hamming_distance
  }
}
```

### All Distance Operators

```graphql
# Cosine distance (semantic similarity)
query { documents(where: { embedding: { cosine_distance: [0.1, 0.2, 0.3] } }) }

# L2 distance (Euclidean)
query { documents(where: { embedding: { l2_distance: [0.1, 0.2, 0.3] } }) }

# Inner product
query { documents(where: { embedding: { inner_product: [0.1, 0.2, 0.3] } }) }

# L1 distance (Manhattan)
query { documents(where: { embedding: { l1_distance: [0.1, 0.2, 0.3] } }) }

# Hamming distance (binary)
query { documents(where: { binary_hash: { hamming_distance: "10101010" } }) }

# Jaccard distance (set similarity)
query { documents(where: { binary_hash: { jaccard_distance: "10101010" } }) }
```

## Distance Semantics

### Float Vector Operators
- **cosine_distance**: 0.0 (identical) to 2.0 (opposite)
- **l2_distance**: 0.0 (identical) to ∞ (very different)
- **inner_product**: More negative = more similar
- **l1_distance**: 0.0 (identical) to ∞ (very different)

### Binary Vector Operators
- **hamming_distance**: 0 (identical) to bit_length (completely different)
- **jaccard_distance**: 0.0 (identical sets) to 1.0 (no overlap)

## Performance Tips

1. **Always create indexes** on vector columns for production use
2. **Use appropriate dimensions** matching your embedding model
3. **Limit results** to control query performance
4. **Filter first** with metadata before vector similarity
5. **Monitor query plans** with `EXPLAIN` to verify index usage

## Sample Data

```sql
-- Insert sample documents with embeddings
INSERT INTO documents (title, content, embedding, binary_hash, category) VALUES
(
  'Machine Learning Basics',
  'Introduction to machine learning concepts...',
  '[0.1, 0.2, 0.3, ...]'::vector,
  B'1010101010101010',
  'technical'
),
(
  'Deep Learning Guide',
  'Advanced neural network techniques...',
  '[0.15, 0.25, 0.35, ...]'::vector,
  B'1010101010101011',
  'technical'
);
```

This example demonstrates how FraiseQL makes vector similarity search as simple as traditional SQL queries while exposing the full power of PostgreSQL's pgvector extension.</content>
</xai:function_call">\
<xai:function_call name="write">
<parameter name="filePath">examples/vector_search/schema.sql
