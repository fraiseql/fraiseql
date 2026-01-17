-- fraiseql-wire Test Database Schema
--
-- This schema provides realistic test data for integration testing.
-- Follows fraiseql naming conventions:
--   tb_{entity} - command side table (storage)
--   v_{entity}  - canonical entity view (JSON data plane)
--   tv_{entity} - projection table (pre-materialized, not used here)

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- Create test schema
CREATE SCHEMA IF NOT EXISTS test;

-- ============================================================================
-- Entity 1: project (Basic JSON structure)
-- ============================================================================

CREATE TABLE IF NOT EXISTS test.tb_project (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
    data JSONB NOT NULL
);

CREATE OR REPLACE VIEW test.v_project AS
SELECT id, data FROM test.tb_project;

CREATE INDEX IF NOT EXISTS idx_tb_project_created_at ON test.tb_project(created_at);

-- ============================================================================
-- Entity 2: user (Moderate JSON structure)
-- ============================================================================

CREATE TABLE IF NOT EXISTS test.tb_user (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
    data JSONB NOT NULL
);

CREATE OR REPLACE VIEW test.v_user AS
SELECT id, data FROM test.tb_user;

CREATE INDEX IF NOT EXISTS idx_tb_user_created_at ON test.tb_user(created_at);

-- ============================================================================
-- Entity 3: task (Complex nested JSON structure)
-- ============================================================================

CREATE TABLE IF NOT EXISTS test.tb_task (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
    data JSONB NOT NULL
);

CREATE OR REPLACE VIEW test.v_task AS
SELECT id, data FROM test.tb_task;

CREATE INDEX IF NOT EXISTS idx_tb_task_created_at ON test.tb_task(created_at);

-- ============================================================================
-- Entity 4: document (Very large JSON objects)
-- ============================================================================

CREATE TABLE IF NOT EXISTS test.tb_document (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
    data JSONB NOT NULL
);

CREATE OR REPLACE VIEW test.v_document AS
SELECT id, data FROM test.tb_document;

CREATE INDEX IF NOT EXISTS idx_tb_document_created_at ON test.tb_document(created_at);

-- ============================================================================
-- Clean up function (for test isolation)
-- ============================================================================

CREATE OR REPLACE FUNCTION test.truncate_all()
RETURNS void AS $$
BEGIN
    TRUNCATE TABLE test.tb_project CASCADE;
    TRUNCATE TABLE test.tb_user CASCADE;
    TRUNCATE TABLE test.tb_task CASCADE;
    TRUNCATE TABLE test.tb_document CASCADE;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Helper function to check row counts (for verification)
-- ============================================================================

CREATE OR REPLACE FUNCTION test.row_counts()
RETURNS TABLE (
    entity_name TEXT,
    row_count BIGINT
) AS $$
BEGIN
    RETURN QUERY
    SELECT 'project'::TEXT, COUNT(*) FROM test.tb_project
    UNION ALL
    SELECT 'user'::TEXT, COUNT(*) FROM test.tb_user
    UNION ALL
    SELECT 'task'::TEXT, COUNT(*) FROM test.tb_task
    UNION ALL
    SELECT 'document'::TEXT, COUNT(*) FROM test.tb_document;
END;
$$ LANGUAGE plpgsql;
