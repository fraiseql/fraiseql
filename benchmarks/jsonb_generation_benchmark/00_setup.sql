-- Benchmark: jsonb_build_object vs row_to_json vs to_jsonb
-- Purpose: Measure real-world performance of different JSONB generation methods
-- Date: 2025-10-16

-- Drop existing tables if they exist
DROP TABLE IF EXISTS tb_user_bench CASCADE;
DROP VIEW IF EXISTS v_user_jsonb_build CASCADE;
DROP VIEW IF EXISTS v_user_row_to_json CASCADE;
DROP VIEW IF EXISTS v_user_to_jsonb CASCADE;
DROP VIEW IF EXISTS v_user_row_to_json_lateral CASCADE;
DROP TABLE IF EXISTS tv_user_jsonb_build CASCADE;
DROP TABLE IF EXISTS tv_user_to_jsonb CASCADE;

-- Create base table with realistic data structure
CREATE TABLE tb_user_bench (
    pk_user INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    identifier TEXT UNIQUE NOT NULL,
    email VARCHAR(255) NOT NULL UNIQUE,
    name VARCHAR(255) NOT NULL,
    bio TEXT,
    avatar_url VARCHAR(500),
    is_active BOOLEAN DEFAULT true,
    roles TEXT[] DEFAULT ARRAY['user'],
    metadata JSONB DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Add indexes for realistic query performance
CREATE INDEX idx_user_bench_id ON tb_user_bench(id);
CREATE INDEX idx_user_bench_identifier ON tb_user_bench(identifier);
CREATE INDEX idx_user_bench_email ON tb_user_bench(email);
CREATE INDEX idx_user_bench_active ON tb_user_bench(is_active);

-- ==============================================================================
-- Approach 1: jsonb_build_object (current FraiseQL pattern)
-- ==============================================================================

CREATE OR REPLACE VIEW v_user_jsonb_build AS
SELECT
    u.id,
    u.identifier,
    jsonb_build_object(
        'id', u.id::text,
        'identifier', u.identifier,
        'email', u.email,
        'name', u.name,
        'bio', u.bio,
        'avatarUrl', u.avatar_url,
        'isActive', u.is_active,
        'roles', u.roles,
        'metadata', u.metadata,
        'createdAt', u.created_at,
        'updatedAt', u.updated_at
    ) AS data
FROM tb_user_bench u;

-- ==============================================================================
-- Approach 2: row_to_json with LATERAL (manual field selection)
-- ==============================================================================

CREATE OR REPLACE VIEW v_user_row_to_json_lateral AS
SELECT
    u.id,
    u.identifier,
    row_to_json(t)::jsonb AS data
FROM tb_user_bench u
CROSS JOIN LATERAL (
    SELECT
        u.id::text AS "id",
        u.identifier,
        u.email,
        u.name,
        u.bio,
        u.avatar_url AS "avatarUrl",
        u.is_active AS "isActive",
        u.roles,
        u.metadata,
        u.created_at AS "createdAt",
        u.updated_at AS "updatedAt"
) t;

-- ==============================================================================
-- Approach 3: row_to_json with subquery (cleaner syntax)
-- ==============================================================================

CREATE OR REPLACE VIEW v_user_row_to_json AS
SELECT
    u.id,
    u.identifier,
    row_to_json((
        SELECT t FROM (
            SELECT
                u.id::text AS "id",
                u.identifier,
                u.email,
                u.name,
                u.bio,
                u.avatar_url AS "avatarUrl",
                u.is_active AS "isActive",
                u.roles,
                u.metadata,
                u.created_at AS "createdAt",
                u.updated_at AS "updatedAt"
        ) t
    ))::jsonb AS data
FROM tb_user_bench u;

-- ==============================================================================
-- Approach 4: to_jsonb (simplest, but snake_case keys)
-- ==============================================================================

CREATE OR REPLACE VIEW v_user_to_jsonb AS
SELECT
    u.id,
    u.identifier,
    to_jsonb(u) - 'pk_user' AS data
FROM tb_user_bench u;

-- ==============================================================================
-- Approach 5: Trinity table with jsonb_build_object GENERATED column
-- ==============================================================================

CREATE TABLE tv_user_jsonb_build (
    pk_user INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    identifier TEXT UNIQUE NOT NULL,

    -- Actual columns
    email VARCHAR(255) NOT NULL,
    name VARCHAR(255) NOT NULL,
    bio TEXT,
    avatar_url VARCHAR(500),
    is_active BOOLEAN DEFAULT true,
    roles TEXT[] DEFAULT ARRAY['user'],
    metadata JSONB DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),

    -- Generated column using jsonb_build_object
    data JSONB GENERATED ALWAYS AS (
        jsonb_build_object(
            'id', id::text,
            'identifier', identifier,
            'email', email,
            'name', name,
            'bio', bio,
            'avatarUrl', avatar_url,
            'isActive', is_active,
            'roles', roles,
            'metadata', metadata,
            'createdAt', created_at,
            'updatedAt', updated_at
        )
    ) STORED
);

-- ==============================================================================
-- Approach 6: Trinity table with to_jsonb GENERATED column
-- ==============================================================================

CREATE TABLE tv_user_to_jsonb (
    pk_user INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    identifier TEXT UNIQUE NOT NULL,

    -- Actual columns
    email VARCHAR(255) NOT NULL,
    name VARCHAR(255) NOT NULL,
    bio TEXT,
    avatar_url VARCHAR(500),
    is_active BOOLEAN DEFAULT true,
    roles TEXT[] DEFAULT ARRAY['user'],
    metadata JSONB DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),

    -- Generated column using to_jsonb
    data JSONB GENERATED ALWAYS AS (
        to_jsonb(tv_user_to_jsonb) - 'data' - 'pk_user'
    ) STORED
);

-- Add indexes to Trinity tables
CREATE INDEX idx_tv_jsonb_build_id ON tv_user_jsonb_build(id);
CREATE INDEX idx_tv_to_jsonb_id ON tv_user_to_jsonb(id);

-- ==============================================================================
-- Seed data (10,000 rows for realistic benchmarking)
-- ==============================================================================

-- Insert into base table (for views)
INSERT INTO tb_user_bench (identifier, email, name, bio, avatar_url, is_active, roles, metadata)
SELECT
    'user_' || i,
    'user' || i || '@example.com',
    'User ' || i,
    'This is a bio for user ' || i || '. It contains some text to simulate realistic data.',
    'https://example.com/avatar/' || i || '.jpg',
    (i % 10) != 0,  -- 90% active
    ARRAY['user', CASE WHEN i % 100 = 0 THEN 'admin' ELSE 'member' END],
    jsonb_build_object(
        'preferences', jsonb_build_object(
            'theme', CASE WHEN i % 2 = 0 THEN 'dark' ELSE 'light' END,
            'notifications', (i % 3 = 0)
        ),
        'stats', jsonb_build_object(
            'posts_count', (i % 50),
            'followers', (i % 100)
        )
    )
FROM generate_series(1, 10000) AS i;

-- Insert into Trinity tables (same data)
INSERT INTO tv_user_jsonb_build (id, identifier, email, name, bio, avatar_url, is_active, roles, metadata)
SELECT id, identifier, email, name, bio, avatar_url, is_active, roles, metadata
FROM tb_user_bench;

INSERT INTO tv_user_to_jsonb (id, identifier, email, name, bio, avatar_url, is_active, roles, metadata)
SELECT id, identifier, email, name, bio, avatar_url, is_active, roles, metadata
FROM tb_user_bench;

-- Analyze tables for optimal query plans
ANALYZE tb_user_bench;
ANALYZE tv_user_jsonb_build;
ANALYZE tv_user_to_jsonb;

-- ==============================================================================
-- Verification: Ensure all approaches produce valid JSONB
-- ==============================================================================

-- Test each view returns data
SELECT 'v_user_jsonb_build' AS view_name, COUNT(*) AS row_count,
       jsonb_typeof(data) AS data_type,
       pg_size_pretty(pg_total_relation_size('v_user_jsonb_build'::regclass)) AS size
FROM v_user_jsonb_build;

SELECT 'v_user_row_to_json_lateral' AS view_name, COUNT(*) AS row_count,
       jsonb_typeof(data) AS data_type,
       pg_size_pretty(pg_total_relation_size('v_user_row_to_json_lateral'::regclass)) AS size
FROM v_user_row_to_json_lateral;

SELECT 'v_user_row_to_json' AS view_name, COUNT(*) AS row_count,
       jsonb_typeof(data) AS data_type,
       pg_size_pretty(pg_total_relation_size('v_user_row_to_json'::regclass)) AS size
FROM v_user_row_to_json;

SELECT 'v_user_to_jsonb' AS view_name, COUNT(*) AS row_count,
       jsonb_typeof(data) AS data_type,
       pg_size_pretty(pg_total_relation_size('v_user_to_jsonb'::regclass)) AS size
FROM v_user_to_jsonb;

SELECT 'tv_user_jsonb_build' AS table_name, COUNT(*) AS row_count,
       jsonb_typeof(data) AS data_type,
       pg_size_pretty(pg_total_relation_size('tv_user_jsonb_build'::regclass)) AS size
FROM tv_user_jsonb_build;

SELECT 'tv_user_to_jsonb' AS table_name, COUNT(*) AS row_count,
       jsonb_typeof(data) AS data_type,
       pg_size_pretty(pg_total_relation_size('tv_user_to_jsonb'::regclass)) AS size
FROM tv_user_to_jsonb;

-- Show sample output from each approach
SELECT 'jsonb_build_object sample:' AS label, data FROM v_user_jsonb_build LIMIT 1;
SELECT 'row_to_json_lateral sample:' AS label, data FROM v_user_row_to_json_lateral LIMIT 1;
SELECT 'row_to_json sample:' AS label, data FROM v_user_row_to_json LIMIT 1;
SELECT 'to_jsonb sample:' AS label, data FROM v_user_to_jsonb LIMIT 1;

\echo 'Setup complete! Tables and views created with 10,000 rows each.'
\echo 'Run the benchmark scripts to compare performance.'
