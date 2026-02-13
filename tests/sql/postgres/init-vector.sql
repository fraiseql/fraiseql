-- PostgreSQL + pgvector Test Database Initialization
--
-- This script creates test views with vector data for testing pgvector operators.

-- Enable pgvector extension
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- ============================================================================
-- Test View: v_embedding
-- ============================================================================

CREATE TABLE IF NOT EXISTS embeddings_test (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    content TEXT NOT NULL,
    embedding vector(3),  -- Small dimension for testing
    metadata JSONB DEFAULT '{}'
);

-- Insert test data (simple 3D vectors for testing)
INSERT INTO embeddings_test (id, content, embedding, metadata) VALUES
    ('00000000-0000-0000-0000-000000000301', 'First document', '[1.0, 0.0, 0.0]', '{"source": "test"}'::jsonb),
    ('00000000-0000-0000-0000-000000000302', 'Second document', '[0.0, 1.0, 0.0]', '{"source": "test"}'::jsonb),
    ('00000000-0000-0000-0000-000000000303', 'Third document', '[0.0, 0.0, 1.0]', '{"source": "test"}'::jsonb),
    ('00000000-0000-0000-0000-000000000304', 'Similar to first', '[0.9, 0.1, 0.0]', '{"source": "test"}'::jsonb),
    ('00000000-0000-0000-0000-000000000305', 'Similar to second', '[0.1, 0.9, 0.0]', '{"source": "test"}'::jsonb)
ON CONFLICT DO NOTHING;

-- Create JSONB view
CREATE OR REPLACE VIEW v_embedding AS
SELECT
    jsonb_build_object(
        'id', id::text,
        'content', content,
        'embedding', embedding::text,
        'metadata', metadata
    ) AS data
FROM embeddings_test;

-- ============================================================================
-- Test View: v_document (for full-text search)
-- ============================================================================

CREATE TABLE IF NOT EXISTS documents_test (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    search_vector tsvector GENERATED ALWAYS AS (to_tsvector('english', title || ' ' || body)) STORED,
    tags TEXT[] DEFAULT '{}'
);

-- Insert test data
INSERT INTO documents_test (id, title, body, tags) VALUES
    ('00000000-0000-0000-0000-000000000401', 'GraphQL Introduction', 'GraphQL is a query language for APIs and a runtime for fulfilling those queries.', ARRAY['graphql', 'api']),
    ('00000000-0000-0000-0000-000000000402', 'Rust Programming', 'Rust is a systems programming language focused on safety and performance.', ARRAY['rust', 'programming']),
    ('00000000-0000-0000-0000-000000000403', 'PostgreSQL Guide', 'PostgreSQL is a powerful open-source relational database system.', ARRAY['postgresql', 'database']),
    ('00000000-0000-0000-0000-000000000404', 'API Design', 'Best practices for designing robust and scalable APIs.', ARRAY['api', 'design'])
ON CONFLICT DO NOTHING;

-- Create JSONB view
CREATE OR REPLACE VIEW v_document AS
SELECT
    jsonb_build_object(
        'id', id::text,
        'title', title,
        'body', body,
        'tags', to_jsonb(tags)
    ) AS data
FROM documents_test;

-- ============================================================================
-- Grants
-- ============================================================================

GRANT SELECT ON v_embedding TO fraiseql_test;
GRANT SELECT ON v_document TO fraiseql_test;

-- ============================================================================
-- Verification
-- ============================================================================

SELECT 'v_embedding' AS view_name, COUNT(*) AS row_count FROM v_embedding
UNION ALL
SELECT 'v_document', COUNT(*) FROM v_document;
