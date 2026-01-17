-- fraiseql-wire Staging Database Schema
--
-- This schema provides realistic test data for load and stress testing.
-- It includes multiple entity types with varying JSON shapes and complexities.

-- Create staging schema
CREATE SCHEMA IF NOT EXISTS test_staging;

-- ============================================================================
-- Entity 1: Projects (Basic JSON structure)
-- ============================================================================

CREATE TABLE IF NOT EXISTS test_staging.projects (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
    data JSONB NOT NULL
);

CREATE VIEW test_staging.v_projects AS
SELECT id, data FROM test_staging.projects;

-- Indexes for query performance
CREATE INDEX IF NOT EXISTS idx_projects_created_at ON test_staging.projects(created_at);

-- ============================================================================
-- Entity 2: Users (Moderate JSON structure)
-- ============================================================================

CREATE TABLE IF NOT EXISTS test_staging.users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
    data JSONB NOT NULL
);

CREATE VIEW test_staging.v_users AS
SELECT id, data FROM test_staging.users;

CREATE INDEX IF NOT EXISTS idx_users_created_at ON test_staging.users(created_at);

-- ============================================================================
-- Entity 3: Tasks (Complex nested JSON structure)
-- ============================================================================

CREATE TABLE IF NOT EXISTS test_staging.tasks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
    data JSONB NOT NULL
);

CREATE VIEW test_staging.v_tasks AS
SELECT id, data FROM test_staging.tasks;

CREATE INDEX IF NOT EXISTS idx_tasks_created_at ON test_staging.tasks(created_at);

-- ============================================================================
-- Entity 4: Large Documents (Very large JSON objects)
-- ============================================================================

CREATE TABLE IF NOT EXISTS test_staging.documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
    data JSONB NOT NULL
);

CREATE VIEW test_staging.v_documents AS
SELECT id, data FROM test_staging.documents;

CREATE INDEX IF NOT EXISTS idx_documents_created_at ON test_staging.documents(created_at);

-- ============================================================================
-- Clean up function (for test isolation)
-- ============================================================================

CREATE OR REPLACE FUNCTION test_staging.truncate_all()
RETURNS void AS $$
BEGIN
    TRUNCATE TABLE test_staging.projects CASCADE;
    TRUNCATE TABLE test_staging.users CASCADE;
    TRUNCATE TABLE test_staging.tasks CASCADE;
    TRUNCATE TABLE test_staging.documents CASCADE;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Helper function to check row counts (for verification)
-- ============================================================================

CREATE OR REPLACE FUNCTION test_staging.row_counts()
RETURNS TABLE (
    table_name TEXT,
    row_count BIGINT
) AS $$
BEGIN
    RETURN QUERY
    SELECT 'projects'::TEXT, COUNT(*) FROM test_staging.projects
    UNION ALL
    SELECT 'users'::TEXT, COUNT(*) FROM test_staging.users
    UNION ALL
    SELECT 'tasks'::TEXT, COUNT(*) FROM test_staging.tasks
    UNION ALL
    SELECT 'documents'::TEXT, COUNT(*) FROM test_staging.documents;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Verification: Confirm schema created successfully
-- ============================================================================

-- This will be checked by tests:
-- SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'test_staging';
-- Should return: 4 (projects, users, tasks, documents)

-- Views are created, can be verified with:
-- SELECT COUNT(*) FROM information_schema.views WHERE table_schema = 'test_staging';
-- Should return: 4 (v_projects, v_users, v_tasks, v_documents)
