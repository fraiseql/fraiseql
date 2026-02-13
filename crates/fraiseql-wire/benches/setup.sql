-- fraiseql-wire integration benchmark test database setup
--
-- This script creates test views with varying data sizes for benchmarking.
-- Run this on a test database:
--
--   psql -U postgres fraiseql_bench < benches/setup.sql
--
-- Clean up with:
--   psql -U postgres -c "DROP DATABASE fraiseql_bench"

-- Create test views with different row counts
-- These simulate the v_{entity} view pattern from FraiseQL

-- ============================================================================
-- Small Dataset: 1,000 rows with simple JSON
-- ============================================================================

CREATE OR REPLACE VIEW v_test_1k AS
SELECT
    jsonb_build_object(
        'id', gen_random_uuid()::text,
        'name', 'Test Project ' || n,
        'status', CASE (n % 3)
            WHEN 0 THEN 'active'
            WHEN 1 THEN 'pending'
            ELSE 'completed'
        END,
        'priority', (n % 10) + 1
    ) AS data
FROM generate_series(1, 1000) AS n;

-- ============================================================================
-- Medium Dataset: 100,000 rows with moderate JSON
-- ============================================================================

CREATE OR REPLACE VIEW v_test_100k AS
SELECT
    jsonb_build_object(
        'id', gen_random_uuid()::text,
        'name', 'Project ' || n,
        'description', 'Lorem ipsum dolor sit amet, consectetur adipiscing elit. ' || n,
        'status', CASE (n % 5)
            WHEN 0 THEN 'active'
            WHEN 1 THEN 'pending'
            WHEN 2 THEN 'completed'
            WHEN 3 THEN 'archived'
            ELSE 'on-hold'
        END,
        'priority', (n % 10) + 1,
        'estimated_cost', (n * 1000)::float,
        'team_size', (n % 20) + 1,
        'created_at', NOW() - ((n % 365) || ' days')::interval,
        'owner_id', gen_random_uuid()::text
    ) AS data
FROM generate_series(1, 100000) AS n;

-- ============================================================================
-- Large Dataset: 1,000,000 rows (for streaming stability)
-- ============================================================================

CREATE OR REPLACE VIEW v_test_1m AS
SELECT
    jsonb_build_object(
        'id', gen_random_uuid()::text,
        'name', 'Item ' || n,
        'status', CASE (n % 4)
            WHEN 0 THEN 'active'
            WHEN 1 THEN 'inactive'
            WHEN 2 THEN 'pending'
            ELSE 'archived'
        END,
        'value', (n * 100)::float,
        'sequence', n
    ) AS data
FROM generate_series(1, 1000000) AS n;

-- ============================================================================
-- Complex JSON: Nested structures with arrays and objects
-- ============================================================================

CREATE OR REPLACE VIEW v_test_complex_json AS
SELECT
    jsonb_build_object(
        'id', gen_random_uuid()::text,
        'project', jsonb_build_object(
            'id', gen_random_uuid()::text,
            'name', 'Project ' || n,
            'owner', jsonb_build_object(
                'id', gen_random_uuid()::text,
                'name', 'Owner ' || (n % 100),
                'email', 'owner' || (n % 100) || '@example.com'
            ),
            'team', jsonb_build_array(
                jsonb_build_object('id', gen_random_uuid()::text, 'name', 'Alice'),
                jsonb_build_object('id', gen_random_uuid()::text, 'name', 'Bob'),
                jsonb_build_object('id', gen_random_uuid()::text, 'name', 'Charlie')
            )
        ),
        'timeline', jsonb_build_object(
            'start', '2024-01-01T00:00:00Z',
            'end', '2024-12-31T23:59:59Z',
            'milestones', jsonb_build_array(
                jsonb_build_object('date', '2024-03-15', 'name', 'Phase 1'),
                jsonb_build_object('date', '2024-06-15', 'name', 'Phase 2'),
                jsonb_build_object('date', '2024-09-15', 'name', 'Phase 3')
            )
        ),
        'metadata', jsonb_build_object(
            'tags', jsonb_build_array('important', 'high-priority', 'client-facing'),
            'custom_fields', jsonb_build_object(
                'external_id', 'EXT-' || n,
                'business_unit', 'ENG-' || (n % 5),
                'cost_center', 'CC-' || (n % 10)
            )
        )
    ) AS data
FROM generate_series(1, 10000) AS n;

-- ============================================================================
-- Large JSON Payloads: Individual rows > 100KB
-- ============================================================================

CREATE OR REPLACE VIEW v_test_large_payloads AS
SELECT
    jsonb_build_object(
        'id', gen_random_uuid()::text,
        'payload', string_agg(
            'data_' || i || ':' || gen_random_uuid()::text,
            ','
        ),
        'metadata', jsonb_build_object(
            'size_bytes', 102400,
            'record_number', n
        )
    ) AS data
FROM generate_series(1, 100) AS n,
     generate_series(1, 1000) AS i
GROUP BY n;

-- ============================================================================
-- Filtered Views: Simulating SQL predicate effectiveness
-- ============================================================================

CREATE OR REPLACE VIEW v_test_100k_active AS
SELECT data
FROM v_test_100k
WHERE data->>'status' = 'active';

CREATE OR REPLACE VIEW v_test_100k_high_priority AS
SELECT data
FROM v_test_100k
WHERE (data->>'priority')::int >= 8;

CREATE OR REPLACE VIEW v_test_100k_expensive AS
SELECT data
FROM v_test_100k
WHERE (data->>'estimated_cost')::float > 50000;

-- ============================================================================
-- Index on JSON data (simulating production database)
-- ============================================================================

CREATE INDEX IF NOT EXISTS idx_test_100k_status
    ON (SELECT NULL) WHERE FALSE;  -- Placeholder for jsonb index

CREATE INDEX IF NOT EXISTS idx_test_100k_priority
    ON (SELECT NULL) WHERE FALSE;  -- Placeholder for jsonb index

-- ============================================================================
-- Helper function for benchmark data generation
-- ============================================================================

CREATE OR REPLACE FUNCTION generate_benchmark_row(count_seed INT)
RETURNS jsonb AS $$
BEGIN
    RETURN jsonb_build_object(
        'id', gen_random_uuid()::text,
        'sequence', count_seed,
        'name', 'Benchmark ' || count_seed,
        'status', CASE (count_seed % 5)
            WHEN 0 THEN 'active'
            WHEN 1 THEN 'inactive'
            WHEN 2 THEN 'pending'
            WHEN 3 THEN 'completed'
            ELSE 'archived'
        END,
        'value', count_seed * 100,
        'created_at', NOW()
    );
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Verification queries
-- ============================================================================

-- Verify test data is loaded
-- SELECT COUNT(*) FROM v_test_1k;      -- Should be 1,000
-- SELECT COUNT(*) FROM v_test_100k;    -- Should be 100,000
-- SELECT COUNT(*) FROM v_test_1m;      -- Should be 1,000,000
-- SELECT OCTET_LENGTH(data::text) FROM v_test_complex_json LIMIT 1;
-- SELECT COUNT(*) FROM v_test_100k_active;  -- Should be ~20,000 (1 of 5)
