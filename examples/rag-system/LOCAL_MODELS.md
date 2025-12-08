# Using Local Models with RAG System

This guide shows how to use the RAG system with local embedding models instead of OpenAI.

## Why Local Models?

**Benefits:**
- ✅ **No API costs** - Run entirely on your hardware
- ✅ **Data privacy** - All processing stays local
- ✅ **Faster for small documents** - No network latency
- ✅ **Offline capable** - Works without internet
- ✅ **Unlimited usage** - No rate limits or quotas

**Trade-offs:**
- ⚠️ Requires GPU for good performance
- ⚠️ Different embedding dimensions (384 vs 1536)
- ⚠️ Slightly lower quality than OpenAI ada-002
- ⚠️ Need to manage model downloads

## Quick Start

### Option 1: Local Embeddings Only (Recommended)

Use sentence-transformers for embeddings on your GPU:

```bash
# Install dependencies
cd examples/rag-system
pip install -r requirements.txt

# Run with local embeddings
export USE_LOCAL_EMBEDDINGS="true"
export DATABASE_URL="postgresql://localhost:5432/ragdb"
python app.py
```

The app will automatically download and use `sentence-transformers/all-MiniLM-L6-v2` (384 dimensions).

### Option 2: With vLLM Server

If you have a vLLM server running for LLM inference:

```bash
# Your vLLM server should be running on port 8000
# The RAG app will use port 8001 to avoid conflicts

export USE_LOCAL_EMBEDDINGS="true"
export LOCAL_MODEL_URL="http://localhost:8000/v1"
python app.py
```

### Option 3: Docker with Local Models

```bash
# Set environment variables
export USE_LOCAL_EMBEDDINGS="true"

# Run with docker-compose (app exposes on port 8001)
docker compose up -d

# The container will download models on first run
# Models are cached in /root/.cache/huggingface inside container
```

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `USE_LOCAL_EMBEDDINGS` | `false` | Set to `true` to use local models |
| `LOCAL_MODEL_URL` | `http://localhost:8000/v1` | vLLM server URL for LLM |
| `LOCAL_EMBEDDING_MODEL` | `sentence-transformers/all-MiniLM-L6-v2` | Embedding model name |
| `EMBEDDING_DEVICE` | `cuda` | Device for embeddings (`cuda` or `cpu`) |
| `OPENAI_API_KEY` | - | Optional: falls back to OpenAI if set |

### Supported Embedding Models

The system uses sentence-transformers. Popular models:

| Model | Dimensions | Speed | Quality | Use Case |
|-------|------------|-------|---------|----------|
| `all-MiniLM-L6-v2` | 384 | ⚡⚡⚡ Fast | ⭐⭐⭐ Good | General purpose (default) |
| `all-mpnet-base-v2` | 768 | ⚡⚡ Medium | ⭐⭐⭐⭐ Better | Higher quality |
| `multi-qa-mpnet-base-dot-v1` | 768 | ⚡⚡ Medium | ⭐⭐⭐⭐ Better | Q&A optimized |
| `paraphrase-multilingual-mpnet-base-v2` | 768 | ⚡ Slow | ⭐⭐⭐⭐ Better | Multilingual |

**To change model:**

```bash
export LOCAL_EMBEDDING_MODEL="sentence-transformers/all-mpnet-base-v2"
python app.py
```

**Note:** Different models have different dimensions. You'll need to update your PostgreSQL schema:

```sql
-- For 768-dimensional models
ALTER TABLE tv_document_embedding
ALTER COLUMN embedding TYPE vector(768);
```

## Performance Comparison

**⚠️ Note:** The numbers below are **typical benchmarks from public sources**, not tested on your specific system. Actual performance will vary based on hardware, batch size, and document length.

### Embedding Generation Speed (Typical Benchmarks)

**Reference Environment:** NVIDIA RTX 3090 (24GB VRAM)

| Model | Batch Size | Typical Speed | Typical Memory |
|-------|------------|---------------|----------------|
| OpenAI ada-002 (API) | 1 | ~200ms/doc | N/A (API) |
| all-MiniLM-L6-v2 (local) | 32 | ~3ms/doc | ~2GB VRAM |
| all-mpnet-base-v2 (local) | 32 | ~8ms/doc | ~3GB VRAM |

**Source:** sentence-transformers documentation and community benchmarks

**Winner (typically):** Local models can be **50-100x faster** with GPU batching!

### Search Quality (Typical Benchmarks)

**Source:** MTEB (Massive Text Embedding Benchmark) - public leaderboard

| Model | Precision@5 | Recall@10 | Notes |
|-------|-------------|-----------|-------|
| OpenAI ada-002 | ~0.92 | ~0.88 | Best quality |
| all-mpnet-base-v2 | ~0.89 | ~0.85 | Very good |
| all-MiniLM-L6-v2 | ~0.84 | ~0.80 | Good enough |

**Verdict (typical):** Local models are 5-10% lower quality but **much faster and free**.

**To benchmark on your system:** Run the included benchmark script to get real measurements for your hardware and data.

## Architecture

### With Local Embeddings

```
┌─────────────────────────────────────────────────────┐
│  RAG Application (Port 8001)                         │
│                                                       │
│  ┌──────────────────────────────────────────────┐  │
│  │ FastAPI + FraiseQL                            │  │
│  │                                                │  │
│  │  ┌──────────────┐      ┌─────────────────┐  │  │
│  │  │ GraphQL API  │      │ REST API        │  │  │
│  │  └──────────────┘      └─────────────────┘  │  │
│  │          │                      │            │  │
│  │          └──────────┬───────────┘            │  │
│  │                     │                        │  │
│  │          ┌──────────▼──────────┐            │  │
│  │          │ RAGService          │            │  │
│  │          │                     │            │  │
│  │          │  ┌───────────────┐ │            │  │
│  │          │  │ Local         │ │            │  │
│  │          │  │ Embeddings    │ │            │  │
│  │          │  │ (GPU)         │ │            │  │
│  │          │  └───────────────┘ │            │  │
│  │          └─────────────────────┘            │  │
│  └──────────────────┬───────────────────────────┘  │
│                     │                               │
└─────────────────────┼───────────────────────────────┘
                      │
                      ▼
         ┌────────────────────────┐
         │  PostgreSQL + pgvector │
         │  (Vector Storage)      │
         └────────────────────────┘
```

### With vLLM Server (Optional)

```
┌──────────────────────────────────────────────────────┐
│  Host Machine                                         │
│                                                        │
│  ┌────────────────────┐    ┌─────────────────────┐  │
│  │ vLLM Server        │    │ RAG App             │  │
│  │ Port: 8000         │◄───│ Port: 8001          │  │
│  │                    │    │                     │  │
│  │ • Text generation  │    │ • Embeddings (GPU)  │  │
│  │ • Mistral-8B       │    │ • GraphQL/REST      │  │
│  └────────────────────┘    └──────────┬──────────┘  │
│                                        │              │
│                                        ▼              │
│                              ┌──────────────────┐    │
│                              │ PostgreSQL       │    │
│                              │ + pgvector       │    │
│                              └──────────────────┘    │
└──────────────────────────────────────────────────────┘
```

## Docker Setup

### Dockerfile with GPU Support

The current Dockerfile uses CPU. For GPU support:

```dockerfile
FROM pytorch/pytorch:2.1.0-cuda11.8-cudnn8-runtime

WORKDIR /app

# Install system dependencies
RUN apt-get update && apt-get install -y \
    postgresql-client \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy and install requirements
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Copy application
COPY . .

# Download models at build time (optional, for faster startup)
RUN python -c "from sentence_transformers import SentenceTransformer; SentenceTransformer('sentence-transformers/all-MiniLM-L6-v2')"

EXPOSE 8000
CMD ["python", "app.py"]
```

### Docker Compose with GPU

```yaml
version: '3.8'

services:
  rag-app:
    build: .
    deploy:
      resources:
        reservations:
          devices:
            - driver: nvidia
              count: 1
              capabilities: [gpu]
    environment:
      USE_LOCAL_EMBEDDINGS: "true"
      EMBEDDING_DEVICE: "cuda"
    ports:
      - "8001:8000"
```

**Note:** Requires nvidia-docker2 installed.

## Testing

### Test Local Embeddings

```bash
# Test the local embeddings module
cd examples/rag-system
python local_embeddings.py

# Expected output:
# === Testing Sentence Transformers ===
# ✓ Loaded local embedding model: sentence-transformers/all-MiniLM-L6-v2
#   Device: cuda
#   Dimensions: 384
# Text: This is a test document
# Embedding dimensions: 384
# First 5 values: [0.123, -0.456, ...]
```

### Run E2E Tests Without OpenAI

```bash
# Set environment for local models
export USE_LOCAL_EMBEDDINGS="true"

# Run automated tests
./test-rag-system.sh

# All tests should pass using local embeddings
```

## Migration Guide

### From OpenAI to Local

**1. Schema Update**

OpenAI uses 1536 dimensions, local models typically use 384 or 768:

```sql
-- Check current embeddings
SELECT
    embedding_model,
    ARRAY_LENGTH(embedding, 1) as dimensions,
    COUNT(*) as count
FROM tv_document_embedding
GROUP BY embedding_model;

-- Create new embedding table for local models
CREATE TABLE tv_document_embedding_local (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    document_id UUID NOT NULL REFERENCES tb_document(id) ON DELETE CASCADE,
    embedding vector(384),  -- or 768 for larger models
    embedding_model TEXT NOT NULL DEFAULT 'all-MiniLM-L6-v2',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create index
CREATE INDEX ON tv_document_embedding_local
USING hnsw (embedding vector_cosine_ops);
```

**2. Re-generate Embeddings**

```bash
# Script to regenerate embeddings with local model
curl -X POST "http://localhost:8001/api/documents/regenerate-embeddings" \
  -H "Content-Type: application/json" \
  -d '{"model": "local"}'
```

**3. Update Application Config**

```bash
# Update .env
USE_LOCAL_EMBEDDINGS=true
LOCAL_EMBEDDING_MODEL=sentence-transformers/all-MiniLM-L6-v2
```

### From Local to OpenAI

Simply set `OPENAI_API_KEY` and unset `USE_LOCAL_EMBEDDINGS`:

```bash
unset USE_LOCAL_EMBEDDINGS
export OPENAI_API_KEY="your-key"
python app.py
```

The app will automatically use OpenAI.

## Troubleshooting

### Model Download Fails

```bash
# Pre-download models manually
python -c "
from sentence_transformers import SentenceTransformer
model = SentenceTransformer('sentence-transformers/all-MiniLM-L6-v2')
print('Model downloaded successfully!')
"
```

### CUDA Out of Memory

```bash
# Use smaller batch sizes or CPU
export EMBEDDING_DEVICE="cpu"
python app.py
```

### Slow Performance on CPU

CPU inference is 10-50x slower than GPU. Consider:

1. Use smaller model (all-MiniLM-L6-v2)
2. Batch documents
3. Cache frequently-searched queries
4. Or use OpenAI API instead

## Cost Analysis (Typical Estimates)

**⚠️ Note:** These are **typical cost estimates** based on public pricing and average usage patterns. Your actual costs will vary.

### OpenAI API Costs (Current Pricing)

- **Embeddings**: $0.10 per 1M tokens (~750K documents, typical)
- **LLM (GPT-3.5)**: $0.50 per 1M input tokens

**Example:** 10K documents, 1K searches/day (estimated)

- Initial embedding: 10K docs × 200 tokens = 2M tokens = **$0.20**
- Daily searches: 1K × 50 tokens = 50K tokens = **$0.005/day** = **$1.50/month**
- **Total: ~$2/month** (low volume estimate)

### Local Model Costs (Estimated)

- **Hardware**: RTX 3090 (~$1500 one-time, or ~$0.50/hour cloud GPU)
- **Electricity**: ~300W × $0.12/kWh = ~$0.036/hour (varies by location)
- **Maintenance**: Minimal

**Example:** Same 10K documents, 1K searches/day (estimated)

- Initial embedding: Free (~5 minutes on GPU, estimate)
- Daily searches: Free (near-instant on GPU)
- **Operating cost: ~$0.86/day** (if buying GPU) or **~$12/day** (cloud GPU)

**Break-even (rule of thumb):** Local typically makes sense if:
- You already have a GPU
- High volume (>100K searches/month)
- Data privacy requirements

## Best Practices

1. **Development**: Use local models (faster iteration)
2. **Staging**: Test with both local and OpenAI
3. **Production**: Choose based on volume and budget
   - Low volume (< 10K queries/month): OpenAI
   - High volume or privacy needs: Local

4. **Hybrid Approach**: Use local for embeddings, OpenAI for LLM
   - Embeddings are 90% of the cost
   - LLM quality matters more

5. **Monitor Quality**: Track search precision/recall
   - Local models may need tuning for your domain
   - Consider fine-tuning on your data

## Resources

- [sentence-transformers Documentation](https://www.sbert.net/)
- [Hugging Face Model Hub](https://huggingface.co/sentence-transformers)
- [pgvector Performance Guide](https://github.com/pgvector/pgvector#performance)
- [vLLM Documentation](https://docs.vllm.ai/)

## Next Steps

1. ✅ Set up local embeddings
2. ⏭️ Test with your documents
3. ⏭️ Compare quality vs OpenAI
4. ⏭️ Optimize for your use case
5. ⏭️ Deploy to production

Happy embedding! 🚀
