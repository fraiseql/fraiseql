-- pgbench test: Paginated queries (realistic GraphQL pattern)
-- Usage: pgbench -f 03_test_paginated.sql -c 10 -j 4 -T 30 -P 5 dbname

\set offset random(0, 9900)

-- Select 100 rows with offset (typical pagination)
SELECT id, identifier, data
FROM v_user_jsonb_build
ORDER BY id
LIMIT 100 OFFSET :offset;
