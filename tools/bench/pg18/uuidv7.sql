-- #1469: uuidv7() against gen_random_uuid() as an id default, on FraiseQL's shapes.
--
-- Inserts :n rows in committed batches of 1 000 (a write workload, not one bulk statement,
-- so random-key page splits and full-page WAL images show as they do in production) and
-- reports, per (shape, generator): wall time, rows/s, WAL bytes, and the uuid index's size.
--
-- Shapes:
--   entity — a Trinity table: `pk bigint` identity primary key, `id uuid UNIQUE`, `data jsonb`
--            (what `fraiseql init` and the observer tables ship).
--   log    — `id uuid PRIMARY KEY`, payload, timestamp (audit log, tenants, RBAC, DLQ).
--
-- Usage: psql "$DATABASE_URL" -v n=1000000 -f uuidv7.sql

\set ON_ERROR_STOP on
DROP SCHEMA IF EXISTS bench_pg18_uuid CASCADE;
CREATE SCHEMA bench_pg18_uuid;
SET search_path = bench_pg18_uuid;

CREATE TABLE result (
    shape text, generator text, rows bigint, seconds numeric, wal_bytes numeric, index_bytes bigint
);

CREATE PROCEDURE run(shape text, generator text, n bigint)
LANGUAGE plpgsql AS $$
DECLARE
    started   timestamptz := clock_timestamp();
    lsn       pg_lsn := pg_current_wal_lsn();
    done      bigint := 0;
    batch     constant int := 1000;
    index_rel text;
BEGIN
    EXECUTE 'DROP TABLE IF EXISTS t';
    IF shape = 'entity' THEN
        EXECUTE format(
            'CREATE TABLE t (pk bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, '
            'id uuid NOT NULL DEFAULT %s UNIQUE, data jsonb NOT NULL)', generator);
        index_rel := 't_id_key';
    ELSE
        EXECUTE format(
            'CREATE TABLE t (id uuid PRIMARY KEY DEFAULT %s, payload jsonb NOT NULL, '
            'created_at timestamptz NOT NULL DEFAULT now())', generator);
        index_rel := 't_pkey';
    END IF;
    COMMIT;
    started := clock_timestamp();
    lsn := pg_current_wal_lsn();
    WHILE done < n LOOP
        IF shape = 'entity' THEN
            INSERT INTO t (data)
            SELECT jsonb_build_object('name', 'Product ' || i, 'price', i % 1000)
            FROM generate_series(done + 1, LEAST(done + batch, n)) AS i;
        ELSE
            INSERT INTO t (payload)
            SELECT jsonb_build_object('event', 'update', 'seq', i)
            FROM generate_series(done + 1, LEAST(done + batch, n)) AS i;
        END IF;
        COMMIT;
        done := LEAST(done + batch, n);
    END LOOP;
    INSERT INTO result VALUES (
        shape, generator, n,
        round(extract(epoch FROM clock_timestamp() - started)::numeric, 2),
        pg_wal_lsn_diff(pg_current_wal_lsn(), lsn),
        pg_relation_size(index_rel::regclass));
    COMMIT;
END $$;

CHECKPOINT;
CALL run('entity', 'gen_random_uuid()', :n);
CHECKPOINT;
CALL run('entity', 'uuidv7()', :n);
CHECKPOINT;
CALL run('log', 'gen_random_uuid()', :n);
CHECKPOINT;
CALL run('log', 'uuidv7()', :n);

SELECT shape, generator, rows, seconds, round(rows / seconds) AS rows_per_s,
       pg_size_pretty(wal_bytes) AS wal, pg_size_pretty(index_bytes) AS uuid_index
FROM result ORDER BY shape, generator;

DROP SCHEMA bench_pg18_uuid CASCADE;
