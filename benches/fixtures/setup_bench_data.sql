-- FraiseQL Benchmark Data Setup Script
--
-- This script creates test data for running FraiseQL benchmarks.
-- It creates tables and views with 1M+ rows for performance testing.
--
-- Usage:
--   createdb fraiseql_bench
--   psql fraiseql_bench < benches/fixtures/setup_bench_data.sql
--
-- Or with Docker:
--   docker exec -i fraiseql-postgres psql -U postgres fraiseql_bench < benches/fixtures/setup_bench_data.sql

-- =============================================================================
-- Cleanup existing objects
-- =============================================================================

DROP VIEW IF EXISTS v_users CASCADE;
DROP VIEW IF EXISTS v_benchmark_data CASCADE;
DROP TABLE IF EXISTS bench_users CASCADE;

-- =============================================================================
-- Create benchmark table
-- =============================================================================

CREATE TABLE bench_users (
    id SERIAL PRIMARY KEY,
    data JSONB NOT NULL
);

-- Create index for JSONB queries
CREATE INDEX idx_bench_users_data ON bench_users USING GIN (data);
CREATE INDEX idx_bench_users_status ON bench_users ((data->>'status'));

-- =============================================================================
-- Generate 1M rows of test data
-- =============================================================================

-- Use generate_series for efficient bulk insert
-- Each row has a JSONB payload similar to real application data

INSERT INTO bench_users (data)
SELECT jsonb_build_object(
    'id', i,
    'name', 'User ' || i,
    'email', 'user' || i || '@example.com',
    'firstName', 'First' || (i % 1000),
    'lastName', 'Last' || (i % 500),
    'status', CASE (i % 4)
        WHEN 0 THEN 'active'
        WHEN 1 THEN 'inactive'
        WHEN 2 THEN 'pending'
        ELSE 'suspended'
    END,
    'score', (random() * 100)::int,
    'created_at', NOW() - (random() * interval '365 days'),
    'updated_at', NOW() - (random() * interval '30 days'),
    'tags', jsonb_build_array(
        'tag' || (i % 10),
        'tag' || ((i + 1) % 10),
        'tag' || ((i + 2) % 10)
    ),
    'metadata', jsonb_build_object(
        'source', CASE (i % 3)
            WHEN 0 THEN 'web'
            WHEN 1 THEN 'mobile'
            ELSE 'api'
        END,
        'version', (i % 5) + 1,
        'premium', (i % 7) = 0
    ),
    'profile', jsonb_build_object(
        'avatar', 'https://example.com/avatars/' || (i % 100) || '.png',
        'bio', 'This is the bio for user ' || i || '. It contains some text to simulate real data.',
        'location', CASE (i % 5)
            WHEN 0 THEN 'New York'
            WHEN 1 THEN 'London'
            WHEN 2 THEN 'Tokyo'
            WHEN 3 THEN 'Paris'
            ELSE 'Sydney'
        END
    )
)
FROM generate_series(1, 1000000) AS i;

-- =============================================================================
-- Create views for benchmarks
-- =============================================================================

-- v_users: Used by adapter_comparison.rs benchmarks
CREATE VIEW v_users AS
SELECT data FROM bench_users;

-- v_benchmark_data: Used by full_pipeline_comparison.rs benchmarks
CREATE VIEW v_benchmark_data AS
SELECT data FROM bench_users;

-- =============================================================================
-- Analyze tables for query optimization
-- =============================================================================

ANALYZE bench_users;

-- =============================================================================
-- Verify data
-- =============================================================================

DO $$
DECLARE
    row_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO row_count FROM bench_users;
    RAISE NOTICE 'Created % rows in bench_users', row_count;

    IF row_count < 1000000 THEN
        RAISE WARNING 'Expected 1000000 rows, got %', row_count;
    ELSE
        RAISE NOTICE 'Benchmark data setup complete!';
    END IF;
END $$;

-- Show sample data
SELECT 'Sample data from v_users:' AS info;
SELECT data FROM v_users LIMIT 3;

-- Show status distribution
SELECT 'Status distribution:' AS info;
SELECT data->>'status' AS status, COUNT(*) AS count
FROM bench_users
GROUP BY data->>'status'
ORDER BY count DESC;
