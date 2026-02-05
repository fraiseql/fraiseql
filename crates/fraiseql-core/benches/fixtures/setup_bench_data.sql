-- Benchmark Data Setup for FraiseQL Adapter Comparison
--
-- This script creates test data for benchmarking PostgresAdapter vs FraiseWireAdapter.
-- It generates 1,000,000 rows with realistic JSONB data for performance testing.
--
-- Usage:
--   createdb fraiseql_bench
--   psql fraiseql_bench < benches/fixtures/setup_bench_data.sql

-- Drop existing objects
DROP TABLE IF EXISTS benchmark_data CASCADE;
DROP VIEW IF EXISTS v_benchmark_data CASCADE;

-- Create benchmark data table
CREATE TABLE benchmark_data (
    id SERIAL PRIMARY KEY,
    data JSONB NOT NULL,
    created_at TIMESTAMP DEFAULT NOW()
);

-- Create index on JSONB data for faster queries
CREATE INDEX idx_benchmark_data_gin ON benchmark_data USING GIN (data);
CREATE INDEX idx_benchmark_status ON benchmark_data ((data->>'status'));
CREATE INDEX idx_benchmark_score ON benchmark_data (((data->>'score')::numeric));

-- Generate 1 million rows of realistic test data
-- This takes ~30-60 seconds depending on hardware
DO $$
DECLARE
    batch_size INT := 10000;
    total_rows INT := 1000000;
    i INT;
    status_values TEXT[] := ARRAY['active', 'inactive', 'pending', 'archived'];
    tags_options TEXT[][] := ARRAY[
        ARRAY['urgent', 'important'],
        ARRAY['low-priority', 'routine'],
        ARRAY['review', 'follow-up'],
        ARRAY['completed', 'verified']
    ];
BEGIN
    RAISE NOTICE 'Generating % rows in batches of %...', total_rows, batch_size;

    FOR batch IN 0..(total_rows / batch_size - 1) LOOP
        INSERT INTO benchmark_data (data)
        SELECT jsonb_build_object(
            'id', batch * batch_size + generate_series,
            'name', 'User ' || (batch * batch_size + generate_series),
            'email', 'user' || (batch * batch_size + generate_series) || '@example.com',
            'status', status_values[(generate_series % 4) + 1],
            'score', (random() * 100)::numeric(5,2),
            'age', 18 + (random() * 60)::int,
            'is_premium', (random() > 0.7),
            'tags', tags_options[(generate_series % 4) + 1],
            'metadata', jsonb_build_object(
                'last_login', NOW() - (random() * interval '365 days'),
                'login_count', (random() * 1000)::int,
                'preferences', jsonb_build_object(
                    'theme', CASE WHEN random() > 0.5 THEN 'dark' ELSE 'light' END,
                    'language', CASE
                        WHEN random() > 0.7 THEN 'en'
                        WHEN random() > 0.5 THEN 'fr'
                        ELSE 'es'
                    END
                )
            ),
            'created_at', NOW() - (random() * interval '730 days'),
            'updated_at', NOW() - (random() * interval '30 days')
        )
        FROM generate_series(1, batch_size);

        IF (batch + 1) % 10 = 0 THEN
            RAISE NOTICE 'Progress: % / % rows (% %%)',
                (batch + 1) * batch_size,
                total_rows,
                ((batch + 1) * batch_size * 100.0 / total_rows)::numeric(5,2);
        END IF;
    END LOOP;

    RAISE NOTICE 'Data generation complete!';
END $$;

-- Create view (required for FraiseQL queries)
CREATE VIEW v_benchmark_data AS
SELECT
    id,
    data,
    created_at
FROM benchmark_data;

-- Analyze tables for query planner
ANALYZE benchmark_data;

-- Display statistics
DO $$
DECLARE
    total_count BIGINT;
    active_count BIGINT;
    avg_score NUMERIC;
    data_size TEXT;
BEGIN
    SELECT COUNT(*) INTO total_count FROM benchmark_data;

    SELECT COUNT(*) INTO active_count
    FROM benchmark_data
    WHERE data->>'status' = 'active';

    SELECT AVG((data->>'score')::numeric) INTO avg_score
    FROM benchmark_data;

    SELECT pg_size_pretty(pg_total_relation_size('benchmark_data')) INTO data_size;

    RAISE NOTICE '';
    RAISE NOTICE '=== Benchmark Data Statistics ===';
    RAISE NOTICE 'Total rows: %', total_count;
    RAISE NOTICE 'Active rows: % (% %%)', active_count, (active_count * 100.0 / total_count)::numeric(5,2);
    RAISE NOTICE 'Average score: %', avg_score;
    RAISE NOTICE 'Table size: %', data_size;
    RAISE NOTICE '';
    RAISE NOTICE 'Run benchmarks with:';
    RAISE NOTICE '  export DATABASE_URL="postgresql:///fraiseql_bench"';
    RAISE NOTICE '  cargo bench --bench adapter_comparison';
    RAISE NOTICE '';
END $$;

-- Sample queries for verification
\echo '=== Sample Data (first 3 rows) ==='
SELECT
    id,
    data->>'name' as name,
    data->>'email' as email,
    data->>'status' as status,
    data->>'score' as score
FROM v_benchmark_data
LIMIT 3;

\echo ''
\echo '=== Status Distribution ==='
SELECT
    data->>'status' as status,
    COUNT(*) as count,
    ROUND(COUNT(*) * 100.0 / SUM(COUNT(*)) OVER (), 2) as percentage
FROM v_benchmark_data
GROUP BY data->>'status'
ORDER BY count DESC;
