-- pgbench test: Single row lookup by UUID (most common pattern)
-- Usage: pgbench -f 01_test_single_row.sql -c 10 -j 4 -T 30 -P 5 dbname

\set user_id random(1, 10000)

-- Select a random UUID from the base table
SELECT id FROM tb_user_bench OFFSET :user_id LIMIT 1 \gset

-- Test the specific view/table based on filename suffix
-- This file will be duplicated with different view names

SELECT id, identifier, data
FROM v_user_jsonb_build
WHERE id = :'id';
