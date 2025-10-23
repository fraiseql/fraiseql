-- pgbench test: Full table scan (worst case scenario)
-- Usage: pgbench -f 02_test_full_scan.sql -c 4 -j 2 -T 30 -P 5 dbname

-- Select all rows and retrieve full data column
SELECT id, identifier, data
FROM v_user_jsonb_build;
