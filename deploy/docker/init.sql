-- FraiseQL Database Initialization
-- This script bootstraps roles, extensions, and helper functions.
--
-- ⚠️  SECURITY: the role passwords below are INSECURE PLACEHOLDER DEFAULTS for
--     local/demo use only. A plain `.sql` init script cannot read environment
--     variables, so these literals MUST be replaced before any non-local use —
--     either edit them here for a one-off, or drive role creation from a shell
--     init script (`/docker-entrypoint-initdb.d/*.sh`) that sources them from
--     `${FRAISEQL_APP_PASSWORD:?}` / `${PROMETHEUS_PASSWORD:?}`. The compose
--     stacks that mount this file are demo/test stacks (see #436).

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pg_stat_statements";
CREATE EXTENSION IF NOT EXISTS "pg_buffercache";
CREATE EXTENSION IF NOT EXISTS "pgvector";

-- Create monitoring role for Prometheus (placeholder password — see header)
CREATE ROLE prometheus WITH LOGIN PASSWORD 'prometheus_password';
GRANT pg_monitor TO prometheus;

-- Create application user (placeholder password — see header)
CREATE ROLE fraiseql_app WITH LOGIN PASSWORD 'fraiseql_app_password';
GRANT CONNECT ON DATABASE fraiseql_prod TO fraiseql_app;

-- Grant necessary permissions. The application performs CRUD only — it never
-- needs DDL/owner privileges, so grant the four data verbs rather than ALL.
GRANT USAGE ON SCHEMA public TO fraiseql_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO fraiseql_app;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO fraiseql_app;

-- Set up default privileges for future objects (same narrowed grants)
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO fraiseql_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT USAGE, SELECT ON SEQUENCES TO fraiseql_app;

-- Create vector extension optimized settings
-- These settings are optimized for vector operations
ALTER SYSTEM SET ivfflat.probes = 10;
ALTER SYSTEM SET hnsw.ef_search = 64;

-- Performance monitoring setup
CREATE OR REPLACE FUNCTION get_vector_index_stats()
RETURNS TABLE (
    schemaname name,
    tablename name,
    indexname name,
    idx_scan bigint,
    idx_tup_read bigint,
    idx_tup_fetch bigint
) AS $$
    SELECT
        schemaname,
        tablename,
        indexname,
        idx_scan,
        idx_tup_read,
        idx_tup_fetch
    FROM pg_stat_user_indexes
    WHERE indexname LIKE '%embedding%'
    ORDER BY idx_scan DESC;
-- SECURITY DEFINER runs as the (superuser) owner; pin search_path to pg_catalog so a
-- caller cannot hijack unqualified name resolution to escalate (only pg_catalog objects
-- are referenced here).
$$ LANGUAGE sql SECURITY DEFINER SET search_path = pg_catalog;

-- Health check function
CREATE OR REPLACE FUNCTION health_check()
RETURNS json AS $$
DECLARE
    result json;
BEGIN
    SELECT json_build_object(
        'status', 'healthy',
        'timestamp', now(),
        'database', current_database(),
        'version', version(),
        'connections', (SELECT count(*) FROM pg_stat_activity),
        'vector_enabled', (SELECT count(*) > 0 FROM pg_extension WHERE extname = 'vector')
    ) INTO result;
    RETURN result;
END;
-- SECURITY DEFINER: pin search_path to pg_catalog (search-path-hijack hardening).
$$ LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog;

-- Grant execute permissions
GRANT EXECUTE ON FUNCTION get_vector_index_stats() TO prometheus;
GRANT EXECUTE ON FUNCTION health_check() TO fraiseql_app;

-- Create indexes for common query patterns
-- These will be created when tables are set up by the application

-- Log setup completion
DO $$
BEGIN
    RAISE NOTICE 'FraiseQL production database initialized successfully';
    RAISE NOTICE 'Vector extension: %', (SELECT CASE WHEN count(*) > 0 THEN 'ENABLED' ELSE 'NOT FOUND' END FROM pg_extension WHERE extname = 'vector');
    RAISE NOTICE 'Monitoring extensions: %', (SELECT string_agg(extname, ', ') FROM pg_extension WHERE extname IN ('pg_stat_statements', 'pg_buffercache'));
END $$;
