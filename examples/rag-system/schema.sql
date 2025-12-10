-- RAG System Schema - Trinity Pattern Implementation
-- Documents with embeddings for retrieval-augmented generation

-- Enable pgvector extension for vector operations
CREATE EXTENSION IF NOT EXISTS vector;

-- Trinity Pattern: Table for document storage (Command side)
CREATE TABLE tb_document (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    source TEXT,                    -- Document source (file, URL, etc.)
    metadata JSONB DEFAULT '{}',     -- Additional document metadata
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Trinity Pattern: View for document access (Query side)
CREATE VIEW v_document AS
SELECT
    id,
    title,
    content,
    source,
    metadata,
    created_at,
    updated_at
FROM tb_document;

-- Trinity Pattern: Table view with vector embeddings for semantic search
CREATE TABLE tv_document_embedding (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    document_id UUID NOT NULL REFERENCES tb_document(id) ON DELETE CASCADE,
    embedding vector(1536),          -- OpenAI text-embedding-ada-002 dimensions
    embedding_model TEXT NOT NULL DEFAULT 'text-embedding-ada-002',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Performance indexes
CREATE INDEX ON tb_document (created_at);
CREATE INDEX ON tb_document (source);
CREATE INDEX ON tv_document_embedding (document_id);
CREATE INDEX ON tv_document_embedding USING hnsw (embedding vector_cosine_ops);

-- Function to create document with embedding
CREATE OR REPLACE FUNCTION create_document_with_embedding(
    p_title TEXT,
    p_content TEXT,
    p_source TEXT DEFAULT NULL,
    p_metadata JSONB DEFAULT '{}',
    p_embedding vector(1536) DEFAULT NULL,
    p_embedding_model TEXT DEFAULT 'text-embedding-ada-002'
) RETURNS UUID AS $$
DECLARE
    doc_id UUID;
BEGIN
    -- Insert document
    INSERT INTO tb_document (title, content, source, metadata)
    VALUES (p_title, p_content, p_source, p_metadata)
    RETURNING id INTO doc_id;

    -- Insert embedding if provided
    IF p_embedding IS NOT NULL THEN
        INSERT INTO tv_document_embedding (document_id, embedding, embedding_model)
        VALUES (doc_id, p_embedding, p_embedding_model);
    END IF;

    RETURN doc_id;
END;
$$ LANGUAGE plpgsql;

-- Function to update document embedding
CREATE OR REPLACE FUNCTION update_document_embedding(
    p_document_id UUID,
    p_embedding vector(1536),
    p_embedding_model TEXT DEFAULT 'text-embedding-ada-002'
) RETURNS BOOLEAN AS $$
BEGIN
    -- Delete existing embedding
    DELETE FROM tv_document_embedding WHERE document_id = p_document_id;

    -- Insert new embedding
    INSERT INTO tv_document_embedding (document_id, embedding, embedding_model)
    VALUES (p_document_id, p_embedding, p_embedding_model);

    RETURN TRUE;
END;
$$ LANGUAGE plpgsql;

-- Function for semantic document search
CREATE OR REPLACE FUNCTION search_documents_by_embedding(
    p_query_embedding vector(1536),
    p_limit INTEGER DEFAULT 10,
    p_similarity_threshold REAL DEFAULT 0.7
) RETURNS TABLE (
    id UUID,
    title TEXT,
    content TEXT,
    source TEXT,
    metadata JSONB,
    similarity REAL
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        d.id,
        d.title,
        d.content,
        d.source,
        d.metadata,
        (1 - (e.embedding <=> p_query_embedding))::REAL as similarity
    FROM tb_document d
    JOIN tv_document_embedding e ON d.id = e.document_id
    WHERE (1 - (e.embedding <=> p_query_embedding)) >= p_similarity_threshold
    ORDER BY (e.embedding <=> p_query_embedding)
    LIMIT p_limit;
END;
$$ LANGUAGE plpgsql;

-- Sample documents for testing
INSERT INTO tb_document (title, content, source, metadata) VALUES
(
    'What is FraiseQL?',
    'FraiseQL is a PostgreSQL-first GraphQL framework for the LLM era. It uses a Rust pipeline to transform PostgreSQL JSONB directly to HTTP responses, eliminating Python serialization overhead. The framework follows database-first principles with JSONB views, automatic session variable injection for security, and built-in caching, monitoring, and error tracking - all within PostgreSQL.',
    'documentation',
    '{"category": "technical", "difficulty": "beginner", "tags": ["introduction", "architecture"]}'
),
(
    'FraiseQL Performance Benefits',
    'FraiseQL delivers 10-100x performance improvements through its Rust pipeline that bypasses Python object serialization. Traditional frameworks: PostgreSQL → Rows → ORM → Python objects → GraphQL serialize → JSON. FraiseQL: PostgreSQL → JSONB → Rust field selection → HTTP Response. This eliminates the Python bottleneck while maintaining full GraphQL capabilities.',
    'documentation',
    '{"category": "performance", "difficulty": "intermediate", "tags": ["performance", "rust"]}'
),
(
    'PostgreSQL-Native Features in FraiseQL',
    'FraiseQL implements caching, error tracking, and observability directly in PostgreSQL using UNLOGGED tables for Redis-level performance, automatic error fingerprinting like Sentry, and OpenTelemetry correlation. This "In PostgreSQL Everything" approach saves $300-3,000/month by eliminating external SaaS services while providing ACID guarantees for all observability data.',
    'documentation',
    '{"category": "features", "difficulty": "advanced", "tags": ["postgresql", "monitoring"]}'
),
(
    'Security by Design in FraiseQL',
    'FraiseQL provides security through automatic PostgreSQL session variable injection from JWT tokens. Views automatically filter by tenant_id using current_setting(), making it impossible to query other tenants'' data. The framework uses explicit field contracts to prevent data leaks and implements row-level security at the database level, not application level.',
    'documentation',
    '{"category": "security", "difficulty": "intermediate", "tags": ["security", "multi-tenancy"]}'
),
(
    'JSONB-First Architecture Benefits',
    'FraiseQL embraces PostgreSQL JSONB as a first-class storage mechanism, enabling schema evolution without migrations, flexible tenant-specific data models, and 10-100x faster JSON passthrough performance. Instead of rigid column schemas, FraiseQL uses tb_* tables with JSONB data columns and v_* views that extract fields, providing both flexibility and performance.',
    'documentation',
    '{"category": "architecture", "difficulty": "intermediate", "tags": ["jsonb", "schema"]}'
);
