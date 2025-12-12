# Work Package: Create RAG Example Application

**Package ID:** WP-017
**Assignee Role:** Junior Engineer - Code Examples (ENG-EXAMPLES)
**Priority:** P0 - Critical
**Estimated Hours:** 12 hours
**Dependencies:** None (blocks WP-007)

---

## Objective

Build complete RAG system example for AI/ML engineers.

---

## Deliverables

**New Directory:** `examples/rag-system/`

### Files:
```
examples/rag-system/
├── README.md
├── schema.sql (tb_document, tv_document_embedding)
├── app.py (FastAPI + FraiseQL + LangChain)
├── requirements.txt
└── .env.example
```

---

## Functionality

- Upload documents via GraphQL mutation
- Generate embeddings using LangChain
- Semantic search via GraphQL query
- RAG query answering

---

## Acceptance Criteria

- [ ] Complete working application
- [ ] Uses trinity pattern (tb_document, v_document, tv_document_embedding)
- [ ] Documented in README
- [ ] AI/ML persona can run in <15 min
- [ ] All code tested (no errors)

---

**Deadline:** End of Week 2
