-- pgbench test: Write performance for Trinity tables with GENERATED columns
-- Usage: pgbench -f 05_test_trinity_write.sql -c 10 -j 4 -T 30 -P 5 dbname

\set user_num random(100000, 999999)

-- Insert new user (tests GENERATED column overhead)
INSERT INTO tv_user_jsonb_build (identifier, email, name, bio, avatar_url, is_active, roles, metadata)
VALUES (
    'bench_user_' || :user_num,
    'bench' || :user_num || '@example.com',
    'Bench User ' || :user_num,
    'Benchmark user bio',
    'https://example.com/avatar/' || :user_num || '.jpg',
    true,
    ARRAY['user', 'benchmark'],
    '{"test": true}'::jsonb
)
ON CONFLICT (identifier) DO UPDATE
SET updated_at = NOW()
RETURNING id;
