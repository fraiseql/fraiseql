-- Benchmark: Pure function performance (no joins, no subqueries)
-- Purpose: Measure ONLY the function overhead, not query pattern overhead
-- Date: 2025-10-16

-- Drop existing test views
DROP VIEW IF EXISTS v_pure_jsonb_build CASCADE;
DROP VIEW IF EXISTS v_pure_row_to_json CASCADE;
DROP VIEW IF EXISTS v_pure_to_jsonb CASCADE;

-- Use the same base table from 00_setup.sql
-- Assumes tb_user_bench already exists with 10,000 rows

-- ==============================================================================
-- PURE FUNCTION TESTS: No joins, no subqueries, just the function call
-- ==============================================================================

-- Test 1: jsonb_build_object (direct, no overhead)
CREATE OR REPLACE VIEW v_pure_jsonb_build AS
SELECT
    id,
    identifier,
    jsonb_build_object(
        'id', id::text,
        'identifier', identifier,
        'email', email,
        'name', name,
        'bio', bio,
        'avatar_url', avatar_url,
        'is_active', is_active,
        'roles', roles,
        'metadata', metadata,
        'created_at', created_at,
        'updated_at', updated_at
    ) AS data
FROM tb_user_bench;

-- Test 2: row_to_json with ROW constructor (no LATERAL, no subquery)
CREATE OR REPLACE VIEW v_pure_row_to_json AS
SELECT
    id,
    identifier,
    row_to_json(ROW(
        id::text,
        identifier,
        email,
        name,
        bio,
        avatar_url,
        is_active,
        roles,
        metadata,
        created_at,
        updated_at
    ))::jsonb AS data
FROM tb_user_bench;

-- Test 3: to_jsonb (direct, minimal overhead)
CREATE OR REPLACE VIEW v_pure_to_jsonb AS
SELECT
    id,
    identifier,
    to_jsonb(tb_user_bench) - 'pk_user' AS data
FROM tb_user_bench;

-- ==============================================================================
-- Verification
-- ==============================================================================

SELECT 'v_pure_jsonb_build' AS view_name, COUNT(*) AS row_count
FROM v_pure_jsonb_build;

SELECT 'v_pure_row_to_json' AS view_name, COUNT(*) AS row_count
FROM v_pure_row_to_json;

SELECT 'v_pure_to_jsonb' AS view_name, COUNT(*) AS row_count
FROM v_pure_to_jsonb;

-- Show sample output
SELECT 'pure jsonb_build_object:' AS label, data FROM v_pure_jsonb_build LIMIT 1;
SELECT 'pure row_to_json(ROW(...)):' AS label, data FROM v_pure_row_to_json LIMIT 1;
SELECT 'pure to_jsonb:' AS label, data FROM v_pure_to_jsonb LIMIT 1;

\echo 'Pure function test views created!'
\echo 'These test ONLY the function overhead, no joins or subqueries.'
