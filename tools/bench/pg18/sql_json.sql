-- #1470: the compiler's JSONB read shapes against their SQL/JSON (PG17+) equivalents.
--
-- On :n rows of a `data jsonb` entity table, each pair of statements returns the same rows
-- (checked below, before any timing) and is planned and run with
-- `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` :reps times; the median execution time, the top
-- plan node and the shared buffers touched are reported.
--
-- Usage: psql "$DATABASE_URL" -v n=100000 -v reps=7 -f sql_json.sql

\set ON_ERROR_STOP on
DROP SCHEMA IF EXISTS bench_pg18_json CASCADE;
CREATE SCHEMA bench_pg18_json;
SET search_path = bench_pg18_json;

CREATE TABLE t (pk bigint PRIMARY KEY, data jsonb NOT NULL);
INSERT INTO t
SELECT i, jsonb_build_object(
    'id', md5(i::text), 'name', 'Product ' || i, 'price', (i % 1000) / 10.0,
    'status', (ARRAY['active', 'draft', 'archived'])[1 + i % 3],
    'sku', 'SKU-' || i, 'tags', jsonb_build_array('t' || (i % 50)))
FROM generate_series(1, :n) AS i;
ANALYZE t;

CREATE TABLE result (shape text, variant text, indexed text, median_ms numeric, plan text, buffers bigint);

CREATE FUNCTION measure(shape text, variant text, indexed text, q text, reps int) RETURNS void
LANGUAGE plpgsql AS $$
DECLARE
    plan jsonb; times numeric[] := '{}'; top text; bufs bigint;
BEGIN
    FOR i IN 1..reps LOOP
        EXECUTE 'EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) ' || q INTO plan;
        times := times || (plan->0->>'Execution Time')::numeric;
        top := plan->0->'Plan'->>'Node Type';
        bufs := (plan->0->'Plan'->>'Shared Hit Blocks')::bigint
              + (plan->0->'Plan'->>'Shared Read Blocks')::bigint;
    END LOOP;
    INSERT INTO result VALUES (shape, variant, indexed,
        (SELECT round(percentile_cont(0.5) WITHIN GROUP (ORDER BY x)::numeric, 3) FROM unnest(times) x),
        top, bufs);
END $$;

-- The pairs. Each is checked for equal results first.
CREATE TABLE pair (shape text, current_sql text, sqljson_sql text);
INSERT INTO pair VALUES
 ('projected read (LIMIT 1000)',
  $q$SELECT jsonb_build_object('id', data->>'id', 'name', data->>'name', 'price', data->'price') AS data FROM t ORDER BY pk LIMIT 1000$q$,
  $q$SELECT jsonb_build_object('id', j.id, 'name', j.name, 'price', j.price) AS data FROM t, JSON_TABLE(t.data, '$' COLUMNS (id text PATH '$.id', name text PATH '$.name', price jsonb PATH '$.price')) AS j ORDER BY t.pk LIMIT 1000$q$),
 ('projected read (all rows)',
  $q$SELECT jsonb_build_object('id', data->>'id', 'name', data->>'name', 'price', data->'price') AS data FROM t$q$,
  $q$SELECT jsonb_build_object('id', JSON_VALUE(data, '$.id'), 'name', JSON_VALUE(data, '$.name'), 'price', JSON_QUERY(data, '$.price')) AS data FROM t$q$),
 ('eq filter',
  $q$SELECT data FROM t WHERE data->>'sku' = 'SKU-777'$q$,
  $q$SELECT data FROM t WHERE JSON_VALUE(data, '$.sku') = 'SKU-777'$q$),
 ('containment filter',
  $q$SELECT data FROM t WHERE data @> '{"status": "draft"}'$q$,
  $q$SELECT data FROM t WHERE JSON_EXISTS(data, '$ ? (@.status == "draft")')$q$);

DO $$
DECLARE p record; a bigint; b bigint;
BEGIN
    FOR p IN SELECT * FROM pair LOOP
        EXECUTE format('SELECT count(*) FROM ((%s) EXCEPT ALL (%s)) x', p.current_sql, p.sqljson_sql) INTO a;
        EXECUTE format('SELECT count(*) FROM ((%s) EXCEPT ALL (%s)) x', p.sqljson_sql, p.current_sql) INTO b;
        IF a <> 0 OR b <> 0 THEN
            RAISE EXCEPTION 'pair "%" differs: % / % rows', p.shape, a, b;
        END IF;
    END LOOP;
END $$;

SELECT measure(shape, 'current', 'no', current_sql, :reps) FROM pair;
SELECT measure(shape, 'sql/json', 'no', sqljson_sql, :reps) FROM pair;

-- Indexed: each variant with the index its own expression can use.
CREATE INDEX t_sku_arrow ON t ((data->>'sku'));
CREATE INDEX t_sku_jv ON t ((JSON_VALUE(data, '$.sku')));
CREATE INDEX t_gin ON t USING gin (data jsonb_path_ops);
ANALYZE t;
SELECT measure(shape, 'current', 'yes', current_sql, :reps) FROM pair WHERE shape LIKE '%filter';
SELECT measure(shape, 'sql/json', 'yes', sqljson_sql, :reps) FROM pair WHERE shape LIKE '%filter';

SELECT shape, indexed, variant, median_ms, plan, buffers FROM result ORDER BY shape, indexed, variant;

DROP SCHEMA bench_pg18_json CASCADE;
