# Work Package: Write RAG Tutorial

**Package ID:** WP-007
**Assignee Role:** Technical Writer - API/Examples (TW-API)
**Priority:** P0 - Critical
**Estimated Hours:** 8 hours
**Dependencies:** WP-017 (RAG example app must exist first)

---

## Objective

Create copy-paste ready RAG (Retrieval-Augmented Generation) tutorial using FraiseQL + LangChain + pgvector.

---

## Deliverable

**New File:** `docs/ai-ml/rag-tutorial.md` (60-90 minute tutorial)

---

## Content Outline

```markdown
# Building a RAG System with FraiseQL

**Time to Complete:** 60-90 minutes

## What You'll Build
- Semantic search over documents using pgvector
- LangChain integration for embedding generation
- GraphQL API for querying documents

## Prerequisites
- FraiseQL v1.8.0+
- OpenAI API key (or local embedding model)
- PostgreSQL 14+ with pgvector extension

## Step 1: Install Dependencies
[Copy-paste commands]

## Step 2: Create Database Schema
[Using tb_document, tv_document_embedding]

## Step 3: Generate Embeddings
[LangChain code]

## Step 4: Semantic Search
[GraphQL queries with vector similarity]

## Step 5: RAG Pipeline
[Full LangChain RAG integration]

## Testing
[Expected output, verification]

## Next Steps
[Links to vector-search-guide, embedding-strategies]
```

---

## Acceptance Criteria

- [ ] Copy-paste ready (AI/ML persona completes in <2 hours)
- [ ] Uses trinity pattern (`tb_document`, `tv_document_embedding`)
- [ ] All code from WP-017 example tested
- [ ] Links to WP-008 (vector operators reference)
- [ ] Time estimate accurate

---

**Deadline:** End of Week 2
