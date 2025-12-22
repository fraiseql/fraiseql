# AI/ML Engineer Journey - Build Production RAG Systems

**Time to Complete:** 2 hours
**Prerequisites:** Python proficiency, ML model deployment experience, vector database knowledge
**Goal:** Build a production-ready RAG (Retrieval-Augmented Generation) system with FraiseQL and pgvector

## Overview

As an AI/ML engineer, you need a reliable tech stack for building RAG systems that combine semantic search with LLMs. FraiseQL integrates natively with PostgreSQL's pgvector extension, providing a unified GraphQL API for both traditional data and vector embeddings.

By the end of this journey, you'll have:
- Working RAG system with semantic search
- Understanding of vector operators and indexing
- Production-ready deployment patterns
- Integration with LangChain and OpenAI
- Performance optimization strategies

## Step-by-Step Implementation

### Step 1: Understanding FraiseQL's AI-Native Features (15 minutes)

**Goal:** Learn how FraiseQL treats vectors as first-class citizens

**Read:** [AI-Native Features](../features/ai-native.md)

**Key Concepts:**
- Zero-copy JSONB includes vector types
- pgvector operators exposed in GraphQL
- Automatic embedding column detection
- No custom resolvers needed for vector search

**Why This Matters:**
Traditional GraphQL frameworks require custom resolvers for vector operations. FraiseQL automatically generates vector search queries from your PostgreSQL schema.

**Success Check:** You understand that `ORDER BY embedding <=> query_embedding` becomes a GraphQL query automatically

### Step 2: RAG Tutorial - Hands-On (45 minutes)

**Goal:** Build a complete RAG system from scratch

**Follow:** [RAG Tutorial](../ai-ml/rag-tutorial.md)

**What You'll Build:**
1. Document storage with automatic embedding generation
2. Semantic search using cosine similarity
3. GraphQL API for document retrieval
4. Integration with LangChain for question answering

**Tutorial Steps:**
```bash
# Clone the RAG example
cd examples/rag-system

# Setup database with pgvector
createdb ragdb
psql ragdb < schema.sql

# Install dependencies
pip install -r requirements.txt

# Configure environment
cp .env.example .env
# Edit .env with your OPENAI_API_KEY

# Run the application
python app.py
```

**Expected Outcome:**
- GraphQL API running on http://localhost:8000/graphql
- Ability to add documents and search semantically
- Question-answering endpoint returning contextualized answers

**Success Check:** You can add a document, search for it, and get relevant results

### Step 3: Vector Operators Deep-Dive (20 minutes)

**Goal:** Master pgvector similarity operators

**Read:** [Vector Operators Reference](../reference/vector-operators.md)

**All 6 pgvector Operators:**

| Operator | Name | Use Case | GraphQL Example |
|----------|------|----------|-----------------|
| `<=>` | Cosine Distance | Document similarity (most common) | `orderBy: { embedding_cosine: ASC }` |
| `<->` | L2 Distance | Euclidean distance (images, exact matching) | `orderBy: { embedding_l2: ASC }` |
| `<#>` | Inner Product | Dot product similarity | `orderBy: { embedding_inner: ASC }` |
| `<+>` | L1 Distance | Manhattan distance (sparse vectors) | `orderBy: { embedding_l1: ASC }` |
| `<~>` | Hamming Distance | Binary vectors (hashing) | `orderBy: { embedding_hamming: ASC }` |
| `<%>` | Jaccard Distance | Set similarity (tags, categories) | `orderBy: { embedding_jaccard: ASC }` |

**Choosing the Right Operator:**
- **Text embeddings (OpenAI, Cohere):** Use cosine distance `<=>`
- **Image embeddings (CLIP, ResNet):** Use L2 distance `<->`
- **Normalized embeddings:** Use inner product `<#>` (faster than cosine)
- **Binary hashes:** Use Hamming distance `<~>`

**GraphQL Query Example:**
```graphql
query SearchDocuments($queryEmbedding: [Float!]!) {
  documents(
    orderBy: [{ embedding_cosine: ASC }]
    where: { embedding_cosine: { lt: 0.3 } }  # Similarity threshold
    limit: 10
  ) {
    id
    title
    content
    similarity  # Computed: 1 - cosine_distance
  }
}
```

**Success Check:** You can explain when to use each operator

### Step 4: LangChain Integration (25 minutes)

**Goal:** Integrate FraiseQL with LangChain RAG pipelines

**Read:** [LangChain Integration Guide](../guides/langchain-integration.md)

**Integration Pattern:**

```python
from langchain.vectorstores import FraiseQLVectorStore
from langchain.embeddings import OpenAIEmbeddings
from langchain.chat_models import ChatOpenAI
from langchain.chains import RetrievalQA

# Setup FraiseQL vector store
embeddings = OpenAIEmbeddings()
vectorstore = FraiseQLVectorStore(
    graphql_endpoint="http://localhost:8000/graphql",
    embedding_field="embedding",
    text_field="content"
)

# Create RAG chain
llm = ChatOpenAI(model="gpt-4")
qa_chain = RetrievalQA.from_chain_type(
    llm=llm,
    chain_type="stuff",
    retriever=vectorstore.as_retriever(search_kwargs={"k": 5})
)

# Ask questions
answer = qa_chain.run("How does FraiseQL handle vector search?")
print(answer)
```

**Advantages Over Traditional Vector DBs:**
- ✅ **Unified data model:** Vectors + metadata in one query
- ✅ **ACID transactions:** Consistent updates across tables
- ✅ **SQL join power:** Combine vector search with filters
- ✅ **No separate service:** Simpler architecture, lower latency

**Example: Filtered Vector Search**
```graphql
# Find similar documents by a specific author
query FilteredSearch($queryEmbedding: [Float!]!, $authorId: UUID!) {
  documents(
    where: {
      AND: [
        { author_id: { eq: $authorId } },
        { embedding_cosine: { lt: 0.3 } }
      ]
    },
    orderBy: [{ embedding_cosine: ASC }],
    limit: 10
  ) {
    id
    title
    author { name }
    similarity
  }
}
```

**Success Check:** You understand how to combine filters with vector search

### Step 5: Performance Optimization (20 minutes)

**Goal:** Optimize vector search for production scale

**Read:** [pgvector Performance Guide](../features/pgvector.md)

**Indexing Strategies:**

**1. HNSW Index (Recommended for Production):**
```sql
-- Hierarchical Navigable Small World index
-- Fast approximate search with 95%+ recall
CREATE INDEX ON tv_document_embedding
USING hnsw (embedding vector_cosine_ops)
WITH (m = 16, ef_construction = 64);
```

**Parameters:**
- `m`: Max connections per node (16-64, default 16)
- `ef_construction`: Build quality (64-200, higher = better recall)

**2. IVFFlat Index (For Very Large Datasets):**
```sql
-- Inverted file with flat compression
-- Memory-efficient, good for 1M+ vectors
CREATE INDEX ON tv_document_embedding
USING ivfflat (embedding vector_cosine_ops)
WITH (lists = 1000);  -- sqrt(total_vectors) is good default
```

**Performance Benchmarks:**
| Dataset Size | Index Type | QPS (p95 latency) | Recall |
|--------------|------------|-------------------|--------|
| 10K vectors | HNSW | ~500 qps (<20ms) | 98% |
| 100K vectors | HNSW | ~300 qps (<30ms) | 95% |
| 1M vectors | HNSW | ~150 qps (<50ms) | 92% |
| 10M vectors | IVFFlat | ~50 qps (<100ms) | 85% |

**Optimization Checklist:**
- [ ] Use HNSW index for datasets < 5M vectors
- [ ] Set `ef_search` parameter at query time (higher = better recall)
- [ ] Monitor index build time (can take minutes for large datasets)
- [ ] Use IVFFlat for memory-constrained environments
- [ ] Consider partitioning for datasets > 10M vectors

**Query-Time Tuning:**
```sql
-- Increase recall for critical queries (slower)
SET hnsw.ef_search = 200;  -- Default: 40

-- Decrease for higher throughput (lower recall)
SET hnsw.ef_search = 20;
```

**Success Check:** You can choose the right index and tune parameters

### Step 6: Production Deployment Patterns (15 minutes)

**Goal:** Deploy RAG system to production

**Read:** [Production Deployment Checklist](../production/deployment-checklist.md)

**RAG-Specific Deployment Considerations:**

**1. Embedding Generation Strategy:**
```python
# Option A: Synchronous (simple, blocks API)
async def add_document_with_embedding(content: str):
    embedding = await openai.embeddings.create(
        model="text-embedding-ada-002",
        input=content
    )
    await db.insert("documents", {
        "content": content,
        "embedding": embedding.data[0].embedding
    })

# Option B: Async with queue (production)
async def add_document_async(content: str):
    # Insert document first
    doc_id = await db.insert("documents", {"content": content})

    # Queue embedding generation
    await celery_app.send_task("generate_embedding", args=[doc_id])

    return doc_id
```

**2. Embedding Cache Strategy:**
```python
# Cache frequent query embeddings
@cache(ttl=3600)  # 1 hour
async def get_query_embedding(query_text: str):
    return await openai.embeddings.create(
        model="text-embedding-ada-002",
        input=query_text
    )
```

**3. Monitoring RAG Systems:**
```python
# Track RAG-specific metrics
metrics = {
    "embedding_generation_latency": Histogram(...),
    "vector_search_latency": Histogram(...),
    "llm_call_latency": Histogram(...),
    "retrieval_relevance_score": Gauge(...),  # User feedback
    "cache_hit_rate": Gauge(...)
}
```

**4. Cost Optimization:**
- **Embedding costs:** $0.0001 per 1K tokens (OpenAI ada-002)
  - Cache query embeddings (90% cost reduction)
  - Batch document embedding generation
- **LLM costs:** $0.03 per 1K tokens (GPT-4)
  - Limit context documents (3-5 max)
  - Use gpt-3.5-turbo for non-critical queries

**5. Scaling Considerations:**
```yaml
# Kubernetes deployment
apiVersion: apps/v1
kind: Deployment
metadata:
  name: rag-api
spec:
  replicas: 3
  template:
    spec:
      containers:
      - name: fraiseql
        resources:
          requests:
            memory: "512Mi"
            cpu: "500m"
          limits:
            memory: "2Gi"
            cpu: "2000m"
        env:
        - name: PG_POOL_SIZE
          value: "20"  # Connection pool
        - name: OPENAI_API_KEY
          valueFrom:
            secretKeyRef:
              name: openai-secret
              key: api-key
```

**Success Check:** You have a deployment plan for production RAG

### Step 7: Advanced RAG Patterns (20 minutes)

**Goal:** Implement advanced RAG techniques

**Advanced Techniques:**

**1. Hybrid Search (Semantic + Keyword):**
```sql
-- Combine pgvector with PostgreSQL full-text search
CREATE INDEX ON tb_document USING GIN (to_tsvector('english', content));

-- Query combines both
SELECT d.*,
       (1 - (e.embedding <=> query_embedding)) as semantic_score,
       ts_rank(to_tsvector('english', d.content), query_tsquery) as keyword_score,
       ((1 - (e.embedding <=> query_embedding)) * 0.7 +
        ts_rank(to_tsvector('english', d.content), query_tsquery) * 0.3) as combined_score
FROM tb_document d
JOIN tv_document_embedding e ON d.id = e.document_id
WHERE to_tsvector('english', d.content) @@ query_tsquery
ORDER BY combined_score DESC
LIMIT 10;
```

**2. Hierarchical RAG (Parent-Child Documents):**
```sql
-- Split documents into chunks for better retrieval
CREATE TABLE tb_document_chunk (
    pk_chunk INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    fk_document INT REFERENCES tb_document(pk_document),
    chunk_text TEXT NOT NULL,
    chunk_index INT NOT NULL,
    embedding VECTOR(1536)
);

-- Search chunks, retrieve parent documents
```

**3. Multi-Vector Search (Multiple Embeddings):**
```sql
-- Store multiple embeddings per document (title, summary, content)
CREATE TABLE tv_document_multi_embedding (
    document_id UUID,
    title_embedding VECTOR(1536),
    summary_embedding VECTOR(1536),
    content_embedding VECTOR(1536)
);

-- Query all three, combine scores
```

**4. Temporal RAG (Time-Aware Retrieval):**
```graphql
query TimeAwareSearch($queryEmbedding: [Float!]!, $afterDate: DateTime!) {
  documents(
    where: {
      AND: [
        { created_at: { gte: $afterDate } },
        { embedding_cosine: { lt: 0.3 } }
      ]
    },
    orderBy: [
      { embedding_cosine: ASC },
      { created_at: DESC }
    ],
    limit: 10
  ) {
    id
    title
    created_at
    similarity
  }
}
```

**Success Check:** You can implement advanced RAG patterns as needed

## Production RAG System Summary

**Architecture:** ✅ FraiseQL + PostgreSQL + pgvector + LangChain
**Performance:** ✅ HNSW indexes, <50ms p95 latency for 100K vectors
**Integration:** ✅ Native LangChain support, OpenAI/Cohere embeddings
**Scalability:** ✅ Horizontal scaling, connection pooling, caching
**Cost:** ✅ Optimized with embedding cache and context limits

## Next Steps

### Immediate Actions
1. **Run the RAG example:** `examples/rag-system/`
2. **Experiment with operators:** Try all 6 pgvector operators
3. **Deploy to staging:** Use production checklist

### Advanced Topics
- **Multi-modal RAG:** Combine text and image embeddings
- **Fine-tuning embeddings:** Custom models for domain-specific search
- **Evaluation frameworks:** Measure retrieval quality (recall@k, MRR)
- **RAG observability:** Track retrieval relevance and LLM quality

### Community Resources
- **Discord:** Ask RAG questions in #ai-ml channel
- **Examples:** `examples/rag-system/` - Complete working implementation
- **Blog:** Case studies of production RAG deployments

## Troubleshooting

### Common Issues

**Slow vector search:**
- ✅ Create HNSW index on embedding column
- ✅ Increase `hnsw.ef_search` for better recall
- ✅ Check index is being used: `EXPLAIN ANALYZE your_query`

**Poor retrieval quality:**
- ✅ Verify embeddings are normalized (if using inner product)
- ✅ Try different similarity operators (cosine vs L2)
- ✅ Tune similarity threshold (0.2-0.4 for cosine)
- ✅ Check embedding model matches training data distribution

**High embedding costs:**
- ✅ Cache query embeddings (Redis or in-memory)
- ✅ Batch document embedding generation
- ✅ Consider local embedding models (sentence-transformers)

**LangChain integration issues:**
- ✅ Ensure FraiseQL GraphQL endpoint is accessible
- ✅ Check embedding dimensions match (1536 for ada-002)
- ✅ Verify GraphQL schema includes vector fields

## Summary

You now have:
- ✅ Production-ready RAG system with semantic search
- ✅ Understanding of all 6 pgvector operators
- ✅ LangChain integration patterns
- ✅ Performance optimization strategies
- ✅ Deployment and scaling knowledge
- ✅ Advanced RAG techniques

**Estimated Time to Production:** 1-2 weeks for a team of 2 AI/ML engineers

**Recommended Next Journey:** [DevOps Engineer Journey](./devops-engineer.md) for deployment best practices

---

**Questions?** Join our [Discord community](https://discord.gg/fraiseql) #ai-ml channel
