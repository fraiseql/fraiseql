-- Initialize PostgreSQL extensions for FraiseQL
-- This script runs automatically when the database is first created

-- ============================================================================
-- Standard PostgreSQL Extensions
-- ============================================================================

-- Enable UUID generation (standard PostgreSQL extension)
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- ============================================================================
-- FraiseQL Performance Extensions
-- ============================================================================

-- Enable jsonb_ivm (Incremental View Maintenance)
-- Provides 10-100x faster sync operations for CQRS pattern
-- Source: https://github.com/fraiseql/jsonb_ivm
DO $$
BEGIN
    CREATE EXTENSION IF NOT EXISTS jsonb_ivm;
    RAISE NOTICE '✓ jsonb_ivm extension loaded (incremental sync enabled)';
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING 'jsonb_ivm not available (will use slower fallback)';
END $$;

-- Enable pg_fraiseql_cache (cache invalidation with CASCADE rules)
-- Provides automatic cache invalidation when related data changes
-- Source: https://github.com/fraiseql/pg_fraiseql_cache
DO $$
BEGIN
    CREATE EXTENSION IF NOT EXISTS pg_fraiseql_cache;
    RAISE NOTICE '✓ pg_fraiseql_cache extension loaded (CASCADE invalidation enabled)';
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING 'pg_fraiseql_cache not available (will use fallback)';
END $$;

-- ============================================================================
-- Verification
-- ============================================================================

-- List loaded extensions
DO $$
DECLARE
    ext RECORD;
BEGIN
    RAISE NOTICE '';
    RAISE NOTICE 'Installed extensions:';
    FOR ext IN
        SELECT extname, extversion
        FROM pg_extension
        WHERE extname IN ('uuid-ossp', 'jsonb_ivm', 'pg_fraiseql_cache')
        ORDER BY extname
    LOOP
        RAISE NOTICE '  - %: v%', ext.extname, ext.extversion;
    END LOOP;
    RAISE NOTICE '';
END $$;

-- ============================================================================
-- Schema Setup
-- ============================================================================

-- Create schema for migrations tracking
CREATE SCHEMA IF NOT EXISTS fraiseql_migrations;
