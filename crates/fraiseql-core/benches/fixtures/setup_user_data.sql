-- User Type Benchmark Data Setup for FraiseQL
--
-- This script creates test data representing a GraphQL User type for
-- benchmarking PostgresAdapter vs FraiseWireAdapter.
--
-- Usage:
--   createdb fraiseql_bench
--   psql fraiseql_bench < benches/fixtures/setup_user_data.sql

-- Drop existing objects
DROP TABLE IF EXISTS users CASCADE;
DROP VIEW IF EXISTS v_users CASCADE;

-- Create users table with JSONB data column
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    data JSONB NOT NULL,
    created_at TIMESTAMP DEFAULT NOW()
);

-- Create indexes for performance
CREATE INDEX idx_users_data_gin ON users USING GIN (data);
CREATE INDEX idx_users_email ON users ((data->>'email'));
CREATE INDEX idx_users_status ON users ((data->>'status'));
CREATE INDEX idx_users_created_at ON users ((data->>'createdAt'));

-- Generate 1 million User records
-- This represents a typical GraphQL User type
DO $$
DECLARE
    batch_size INT := 10000;
    total_rows INT := 1000000;
    status_values TEXT[] := ARRAY['active', 'inactive', 'suspended', 'pending'];
    role_values TEXT[] := ARRAY['user', 'admin', 'moderator', 'guest'];
BEGIN
    RAISE NOTICE 'Generating % User records in batches of %...', total_rows, batch_size;

    FOR batch IN 0..(total_rows / batch_size - 1) LOOP
        INSERT INTO users (data)
        SELECT jsonb_build_object(
            -- GraphQL User type fields (camelCase in data, will be transformed)
            'id', batch * batch_size + generate_series,
            'email', 'user' || (batch * batch_size + generate_series) || '@example.com',
            'username', 'user' || (batch * batch_size + generate_series),
            'firstName', 'User',
            'lastName', '#' || (batch * batch_size + generate_series),
            'displayName', 'User #' || (batch * batch_size + generate_series),
            'status', status_values[(generate_series % 4) + 1],
            'role', role_values[(generate_series % 4) + 1],
            'isVerified', (random() > 0.3),
            'isPremium', (random() > 0.7),
            'age', 18 + (random() * 60)::int,
            'score', (random() * 100)::numeric(5,2),
            'loginCount', (random() * 1000)::int,
            'lastLoginAt', (NOW() - (random() * interval '365 days'))::text,
            'createdAt', (NOW() - (random() * interval '730 days'))::text,
            'updatedAt', (NOW() - (random() * interval '30 days'))::text,
            'profile', jsonb_build_object(
                'bio', 'This is user #' || (batch * batch_size + generate_series),
                'avatar', 'https://example.com/avatars/' || (batch * batch_size + generate_series) || '.jpg',
                'website', 'https://user' || (batch * batch_size + generate_series) || '.example.com',
                'timezone', CASE WHEN random() > 0.5 THEN 'America/New_York' ELSE 'Europe/Paris' END,
                'language', CASE
                    WHEN random() > 0.7 THEN 'en'
                    WHEN random() > 0.5 THEN 'fr'
                    ELSE 'es'
                END
            ),
            'preferences', jsonb_build_object(
                'theme', CASE WHEN random() > 0.5 THEN 'dark' ELSE 'light' END,
                'emailNotifications', (random() > 0.5),
                'pushNotifications', (random() > 0.6),
                'twoFactorEnabled', (random() > 0.8)
            ),
            'metadata', jsonb_build_object(
                'ipAddress', '192.168.' || (random() * 255)::int || '.' || (random() * 255)::int,
                'userAgent', 'Mozilla/5.0 (compatible)',
                'signupSource', CASE
                    WHEN random() > 0.7 THEN 'web'
                    WHEN random() > 0.5 THEN 'mobile'
                    ELSE 'api'
                END
            )
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

-- Create view with EXACTLY one column named "data" (required by fraiseql-wire)
-- This matches the FraiseQL schema convention
CREATE VIEW v_users AS
SELECT data FROM users;

-- Analyze for query planner
ANALYZE users;

-- Display statistics
DO $$
DECLARE
    total_count BIGINT;
    active_count BIGINT;
    premium_count BIGINT;
    admin_count BIGINT;
    avg_score NUMERIC;
    data_size TEXT;
BEGIN
    SELECT COUNT(*) INTO total_count FROM users;

    SELECT COUNT(*) INTO active_count
    FROM users
    WHERE data->>'status' = 'active';

    SELECT COUNT(*) INTO premium_count
    FROM users
    WHERE data->>'isPremium' = 'true';

    SELECT COUNT(*) INTO admin_count
    FROM users
    WHERE data->>'role' = 'admin';

    SELECT AVG((data->>'score')::numeric) INTO avg_score
    FROM users;

    SELECT pg_size_pretty(pg_total_relation_size('users')) INTO data_size;

    RAISE NOTICE '';
    RAISE NOTICE '=== User Type Benchmark Data Statistics ===';
    RAISE NOTICE 'Total users: %', total_count;
    RAISE NOTICE 'Active users: % (% %%)', active_count, (active_count * 100.0 / total_count)::numeric(5,2);
    RAISE NOTICE 'Premium users: % (% %%)', premium_count, (premium_count * 100.0 / total_count)::numeric(5,2);
    RAISE NOTICE 'Admin users: % (% %%)', admin_count, (admin_count * 100.0 / total_count)::numeric(5,2);
    RAISE NOTICE 'Average score: %', avg_score;
    RAISE NOTICE 'Table size: %', data_size;
    RAISE NOTICE '';
    RAISE NOTICE 'GraphQL User type fields:';
    RAISE NOTICE '  id, email, username, firstName, lastName, displayName';
    RAISE NOTICE '  status, role, isVerified, isPremium, age, score';
    RAISE NOTICE '  loginCount, lastLoginAt, createdAt, updatedAt';
    RAISE NOTICE '  profile { bio, avatar, website, timezone, language }';
    RAISE NOTICE '  preferences { theme, emailNotifications, pushNotifications, twoFactorEnabled }';
    RAISE NOTICE '  metadata { ipAddress, userAgent, signupSource }';
    RAISE NOTICE '';
    RAISE NOTICE 'Run benchmarks with:';
    RAISE NOTICE '  export DATABASE_URL="postgresql:///fraiseql_bench"';
    RAISE NOTICE '  cargo bench --bench adapter_comparison --features "postgres,wire-backend"';
    RAISE NOTICE '';
END $$;

-- Sample queries for verification
\echo '=== Sample User Data (first 3 users) ==='
SELECT
    data->>'id' as id,
    data->>'email' as email,
    data->>'username' as username,
    data->>'firstName' as first_name,
    data->>'lastName' as last_name,
    data->>'status' as status,
    data->>'role' as role
FROM v_users
LIMIT 3;

\echo ''
\echo '=== Status Distribution ==='
SELECT
    data->>'status' as status,
    COUNT(*) as count,
    ROUND(COUNT(*) * 100.0 / SUM(COUNT(*)) OVER (), 2) as percentage
FROM v_users
GROUP BY data->>'status'
ORDER BY count DESC;

\echo ''
\echo '=== Role Distribution ==='
SELECT
    data->>'role' as role,
    COUNT(*) as count,
    ROUND(COUNT(*) * 100.0 / SUM(COUNT(*)) OVER (), 2) as percentage
FROM v_users
GROUP BY data->>'role'
ORDER BY count DESC;
