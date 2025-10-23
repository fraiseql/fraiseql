-- pgbench test: Filtered queries (typical WHERE clause usage)
-- Usage: pgbench -f 04_test_filtered.sql -c 10 -j 4 -T 30 -P 5 dbname

-- Select active users (90% of dataset)
SELECT id, identifier, data
FROM v_user_jsonb_build
WHERE id IN (
    SELECT id FROM tb_user_bench WHERE is_active = true LIMIT 100
);
