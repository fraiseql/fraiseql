-- #1468: a mutation's after-image read back after the write, against `RETURNING OLD.*, NEW.*`.
--
-- Two function bodies over the same table, with a BEFORE UPDATE trigger that rewrites NEW
-- (normalises the name, stamps updated_at), as FraiseQL's mutation functions are written:
--   current   — UPDATE, then SELECT the row back for the response (two statements);
--   returning — UPDATE … RETURNING OLD.*, NEW.* (one statement).
-- First checks that both return the identical after-image (trigger rewrite included), then
-- times :reps × 200 calls of each, single-row and 100-row batch, server-side
-- (clock_timestamp around each call), and reports the shared buffers each body's statements
-- touch, from EXPLAIN (ANALYZE, BUFFERS) of those statements (a function hides its inner
-- statements' plans, and the statistics counters update only when a top-level statement
-- ends, so neither can be read from inside the timing loop).
--
-- Usage: psql "$DATABASE_URL" -v n=10000 -v reps=7 -f returning.sql

\set ON_ERROR_STOP on
DROP SCHEMA IF EXISTS bench_pg18_ret CASCADE;
CREATE SCHEMA bench_pg18_ret;
SET search_path = bench_pg18_ret;

CREATE TABLE tb_product (
    pk bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id uuid NOT NULL DEFAULT uuidv7() UNIQUE,
    batch int NOT NULL,
    name text NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);
INSERT INTO tb_product (batch, name) SELECT i / 100, 'Product ' || i FROM generate_series(1, :n) AS i;
ANALYZE tb_product;

CREATE FUNCTION normalise() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    NEW.name := initcap(trim(NEW.name));
    NEW.updated_at := clock_timestamp();
    RETURN NEW;
END $$;
CREATE TRIGGER normalise BEFORE UPDATE ON tb_product FOR EACH ROW EXECUTE FUNCTION normalise();

-- current: write, then read the after-image back.
CREATE FUNCTION update_current(p_batch int, p_name text) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE result jsonb;
BEGIN
    UPDATE tb_product SET name = p_name WHERE batch = p_batch;
    SELECT jsonb_agg(jsonb_build_object('id', id, 'name', name, 'updated_at', updated_at) ORDER BY pk)
      INTO result FROM tb_product WHERE batch = p_batch;
    RETURN result;
END $$;

-- returning: one statement hands back both images.
CREATE FUNCTION update_returning(p_batch int, p_name text) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE result jsonb;
BEGIN
    WITH changed AS (
        UPDATE tb_product SET name = p_name WHERE batch = p_batch
        RETURNING NEW.pk, NEW.id, NEW.name, NEW.updated_at, OLD.name AS old_name
    )
    SELECT jsonb_agg(jsonb_build_object('id', id, 'name', name, 'updated_at', updated_at) ORDER BY pk)
      INTO result FROM changed;
    RETURN result;
END $$;

-- Identity: same after-image, trigger rewrite included (timestamps excepted).
DO $$
DECLARE a jsonb; b jsonb;
BEGIN
    a := update_current(3, '  hello world ');
    b := update_returning(3, '  hello world ');
    IF (SELECT jsonb_agg(e - 'updated_at') FROM jsonb_array_elements(a) e)
       IS DISTINCT FROM (SELECT jsonb_agg(e - 'updated_at') FROM jsonb_array_elements(b) e) THEN
        RAISE EXCEPTION 'after-images differ: % / %', a, b;
    END IF;
    IF a->0->>'name' <> 'Hello World' THEN
        RAISE EXCEPTION 'the trigger rewrite is not in the after-image: %', a;
    END IF;
END $$;

CREATE TABLE result (variant text, rows_per_call int, p50_us numeric, p99_us numeric);

CREATE PROCEDURE measure(variant text, rows_per_call int, reps int)
LANGUAGE plpgsql AS $$
DECLARE
    times numeric[] := '{}'; t0 timestamptz; calls int := 0; target int;
BEGIN
    FOR r IN 1..reps LOOP
        FOR c IN 1..200 LOOP
            target := 1 + (c % 90);
            t0 := clock_timestamp();
            IF rows_per_call = 1 THEN
                IF variant = 'current' THEN
                    PERFORM update_current_one(target * 100 + 1, 'name ' || c);
                ELSE
                    PERFORM update_returning_one(target * 100 + 1, 'name ' || c);
                END IF;
            ELSIF variant = 'current' THEN
                PERFORM update_current(target, 'name ' || c);
            ELSE
                PERFORM update_returning(target, 'name ' || c);
            END IF;
            times := times || (extract(epoch FROM clock_timestamp() - t0) * 1e6)::numeric;
            calls := calls + 1;
        END LOOP;
        COMMIT;
    END LOOP;
    INSERT INTO result VALUES (variant, rows_per_call,
        (SELECT round(percentile_cont(0.5) WITHIN GROUP (ORDER BY x)::numeric, 1) FROM unnest(times) x),
        (SELECT round(percentile_cont(0.99) WITHIN GROUP (ORDER BY x)::numeric, 1) FROM unnest(times) x));
    COMMIT;
END $$;

-- Single-row variants, by primary key.
CREATE FUNCTION update_current_one(p_pk bigint, p_name text) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE result jsonb;
BEGIN
    UPDATE tb_product SET name = p_name WHERE pk = p_pk;
    SELECT jsonb_build_object('id', id, 'name', name, 'updated_at', updated_at)
      INTO result FROM tb_product WHERE pk = p_pk;
    RETURN result;
END $$;
CREATE FUNCTION update_returning_one(p_pk bigint, p_name text) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE result jsonb;
BEGIN
    UPDATE tb_product SET name = p_name WHERE pk = p_pk
    RETURNING jsonb_build_object('id', NEW.id, 'name', NEW.name, 'updated_at', NEW.updated_at) INTO result;
    RETURN result;
END $$;

-- A batch is located by `batch`, which needs an index to be a fair comparison.
CREATE INDEX tb_product_batch ON tb_product (batch);
ANALYZE tb_product;

CALL measure('current', 1, :reps);
CALL measure('returning', 1, :reps);
CALL measure('current', 100, :reps);
CALL measure('returning', 100, :reps);

SELECT * FROM result ORDER BY rows_per_call, variant;

-- Shared buffers each body's statements touch, single row and 100-row batch.
CREATE FUNCTION buffers(q text) RETURNS bigint LANGUAGE plpgsql AS $$
DECLARE plan jsonb;
BEGIN
    EXECUTE 'EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) ' || q INTO plan;
    RETURN (plan->0->'Plan'->>'Shared Hit Blocks')::bigint
         + (plan->0->'Plan'->>'Shared Read Blocks')::bigint;
END $$;
SELECT 'current' AS variant, 1 AS rows_per_call,
       buffers($q$UPDATE tb_product SET name = 'b1' WHERE pk = 501$q$)
     + buffers($q$SELECT id, name, updated_at FROM tb_product WHERE pk = 501$q$) AS buffers
UNION ALL
SELECT 'returning', 1,
       buffers($q$UPDATE tb_product SET name = 'b2' WHERE pk = 501 RETURNING NEW.id, NEW.name, NEW.updated_at, OLD.name$q$)
UNION ALL
SELECT 'current', 100,
       buffers($q$UPDATE tb_product SET name = 'b3' WHERE batch = 7$q$)
     + buffers($q$SELECT id, name, updated_at FROM tb_product WHERE batch = 7$q$)
UNION ALL
SELECT 'returning', 100,
       buffers($q$UPDATE tb_product SET name = 'b4' WHERE batch = 7 RETURNING NEW.id, NEW.name, NEW.updated_at, OLD.name$q$);

DROP SCHEMA bench_pg18_ret CASCADE;
