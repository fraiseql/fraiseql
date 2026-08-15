-- Index eligibility for filtered ANN: what `nearest` + `where` actually costs (#959).
--
-- The question this answers is not "is FraiseQL fast". It is whether the HNSW index
-- stays usable when a `nearest` search is combined with a WHERE predicate, and what
-- pgvector 0.8's iterative index scans change — because a filtered ANN query has a
-- failure mode that is not slowness: it returns **fewer rows than asked for**, or the
-- wrong ones, and looks like it worked.
--
-- The shape measured is the one FraiseQL emits: a view exposing the vector as a native
-- column beside a JSONB `data` payload, `ORDER BY <col> <=> '<literal>'::vector LIMIT k`,
-- and the filter as a `data->>'key'` comparison (see docs/operations/vector-search.md).
--
-- Usage:
--
--   createdb / point at any pgvector 0.8+ database, then
--   psql "$DATABASE_URL" -f benches/vector_filtered_ann.sql
--
-- It builds its own schema (`bench_vector`), leaves it behind for re-runs, and prints
-- two tables. Roughly 60 s cold on a warm box, most of it generating the corpus;
-- a re-run against an existing schema measures only.
--
-- Reading the output:
--   * `rows` — how many of the requested 10 came back. Fewer than 10 with matching rows
--     in the table is the filtered-ANN failure mode.
--   * `recall` — overlap with the exact answer (a sequential scan over the same
--     predicate), 0..1. This is the number that decides whether a setting is usable.
--   * `ms` — median of the timed runs.
--
-- What it measured on pgvector 0.8.6 / PostgreSQL 16.14, 2026-08-15 — the numbers
-- quoted in docs/operations/vector-search.md:
--
--   * unfiltered, and at 50% selectivity: recall 1.000 either way, sub-millisecond.
--     `hnsw.iterative_scan` changes nothing, because nothing is being filtered out.
--   * at 5% and below with the default `iterative_scan = off`: **2 to 3 rows come
--     back where 10 were asked for**, recall 0.2. The scan exhausts its candidate
--     list before the filter has yielded k survivors, and the query succeeds.
--   * the same queries with `relaxed_order` or `strict_order`: 10 rows, recall
--     1.000, at 3.1 ms against 0.43 ms. The two orderings were indistinguishable.
--   * a threshold predicate is not index-eligible under any setting, and reading
--     the vector out of the JSONB payload rather than the native column costs
--     **122×** — 2667 ms against 22 ms for the identical predicate.
--
-- Both findings are filed: #1116 (FraiseQL sets neither pgvector GUC, so every
-- deployment is on the under-returning default) and #1117 (the threshold path
-- reads the payload on views that already expose the native column).

\set ON_ERROR_STOP on
\timing off

CREATE EXTENSION IF NOT EXISTS vector;

DO $$
BEGIN
    IF to_regclass('bench_vector.tb_doc') IS NOT NULL THEN
        RAISE NOTICE 'bench_vector.tb_doc exists — reusing it (DROP SCHEMA bench_vector CASCADE to rebuild)';
    END IF;
END
$$;

CREATE SCHEMA IF NOT EXISTS bench_vector;
SET search_path = bench_vector, public;

-- ---------------------------------------------------------------------------
-- Corpus: 100 000 documents, 384 dimensions (the width of a MiniLM-class
-- sentence embedding — large enough to be realistic, small enough that the
-- index fits in memory on a laptop).
--
-- `bucket` splits the corpus into 10 000 equal groups, so a predicate
-- `bucket < N` selects exactly N/10 000 of it. Selectivity is the independent
-- variable here and guessing at it with random categories would blur the one
-- axis the benchmark exists to vary.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS tb_doc (
    id        bigint PRIMARY KEY,
    bucket    int    NOT NULL,
    embedding vector(384) NOT NULL
);

-- The corpus has **cluster structure**, and that is not decoration. Uniformly
-- random points in 384 dimensions have no nearest neighbour worth the name —
-- every pairwise distance concentrates on the same value, and an HNSW index
-- over them reports a recall of about 0.4 that says nothing about any real
-- workload. Real embeddings sit on a low-dimensional manifold, so this builds
-- one: 250 centroids, each document a centroid plus 15% noise.
--
-- The selectivity `bucket` is assigned by id and is therefore **uncorrelated**
-- with the cluster a document falls in. That is the hard case for a
-- post-filtering ANN scan and the general one: a tenant or status predicate is
-- not aligned with embedding space.
--
-- `WHERE g IS NOT NULL` is load-bearing, not noise, and it must reference the
-- **row** key. A sub-select with no outer reference is uncorrelated, PostgreSQL
-- hoists it to an InitPlan and evaluates it once — volatile `random()` and all
-- — so the table gets 100 000 copies of one vector. Correlating on the centroid
-- instead is the same bug one level up: 250 distinct vectors, 400 exact
-- duplicates each. Both versions of this benchmark ran, timed, and printed a
-- plausible table of numbers about a corpus that did not exist. The assertion
-- below is what makes the third version prove itself.
CREATE TEMP TABLE bench_centroid AS
SELECT cid, sub.v AS centroid
FROM generate_series(1, 250) cid,
LATERAL (
    SELECT array_agg(random() * 2 - 1)::vector(384) AS v
    FROM generate_series(1, 384) d
    WHERE cid IS NOT NULL
) sub;

INSERT INTO tb_doc (id, bucket, embedding)
SELECT g, (g - 1) % 10000, doc.v
FROM generate_series(1, 100000) g
JOIN bench_centroid c ON c.cid = (g % 250) + 1,
LATERAL (
    SELECT array_agg(centroid_component + (random() - 0.5) * 0.3)::vector(384) AS v
    FROM unnest(c.centroid::real[]) WITH ORDINALITY AS t(centroid_component, d)
    WHERE g IS NOT NULL
) doc
ON CONFLICT (id) DO NOTHING;

DO $corpus$
DECLARE
    distinct_vectors bigint;
    total            bigint;
BEGIN
    SELECT count(*), count(DISTINCT embedding::text) INTO total, distinct_vectors FROM tb_doc;
    IF distinct_vectors < total THEN
        RAISE EXCEPTION
            'corpus has % rows but only % distinct vectors — the generator was hoisted and '
            'every measurement below would describe a corpus that does not exist',
            total, distinct_vectors;
    END IF;
END
$corpus$;

CREATE OR REPLACE VIEW v_doc AS
    SELECT id,
           jsonb_build_object('id', id, 'bucket', bucket, 'embedding', embedding::text) AS data,
           embedding
    FROM tb_doc;

-- A parallel index build needs more shared memory than a default container has.
SET max_parallel_maintenance_workers = 0;
SET maintenance_work_mem = '1GB';
CREATE INDEX IF NOT EXISTS idx_doc_embedding ON tb_doc USING hnsw (embedding vector_cosine_ops);
ANALYZE tb_doc;

-- ---------------------------------------------------------------------------
-- Measurement
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS bench_result (
    shape       text,
    selectivity text,
    setting     text,
    rows_found  int,
    recall      numeric,
    ms          numeric
);
TRUNCATE bench_result;

DO $bench$
DECLARE
    k            constant int := 10;
    warmups      constant int := 2;
    runs         constant int := 9;
    qvec         vector(384);
    sel          record;
    scan_setting text;
    started      timestamptz;
    samples      numeric[];
    got          bigint[];
    truth        bigint[];
    overlap      int;
    i            int;
BEGIN
    -- One query vector for every shape: a distance is only comparable against the
    -- same question. Taken from the corpus rather than generated, so the exact
    -- answer is guaranteed non-empty at every selectivity.
    SELECT embedding INTO qvec FROM tb_doc WHERE id = 1;

    FOR sel IN
        SELECT * FROM (VALUES
            ('none',    10000),   -- no predicate at all
            ('50%',      5000),
            ('5%',        500),
            ('1%',        100),
            ('0.1%',       10),
            ('0.01%',       1)    -- 10 rows in 100 000 — k of them, exactly
        ) AS t(label, buckets)
    LOOP
        -- Ground truth: the same predicate, ordered exactly. `SET LOCAL
        -- enable_indexscan = off` is what makes it exact — without it the planner
        -- answers from the very index under measurement.
        PERFORM set_config('enable_indexscan', 'off', true);
        SELECT array_agg(id ORDER BY embedding <=> qvec)
        INTO truth
        FROM (
            SELECT id, embedding FROM v_doc
            WHERE (sel.buckets = 10000 OR (data->>'bucket')::int < sel.buckets)
            ORDER BY embedding <=> qvec
            LIMIT k
        ) exact_rows;
        PERFORM set_config('enable_indexscan', 'on', true);

        FOREACH scan_setting IN ARRAY ARRAY['off', 'relaxed_order', 'strict_order']
        LOOP
            PERFORM set_config('hnsw.iterative_scan', scan_setting, true);
            samples := ARRAY[]::numeric[];

            FOR i IN 1 .. warmups + runs LOOP
                started := clock_timestamp();
                SELECT array_agg(id ORDER BY ord)
                INTO got
                FROM (
                    SELECT id, row_number() OVER () AS ord
                    FROM v_doc
                    WHERE (sel.buckets = 10000 OR (data->>'bucket')::int < sel.buckets)
                    ORDER BY embedding <=> qvec
                    LIMIT k
                ) hits;
                IF i > warmups THEN
                    samples := samples
                        || EXTRACT(EPOCH FROM clock_timestamp() - started)::numeric * 1000;
                END IF;
            END LOOP;

            SELECT count(*) INTO overlap
            FROM unnest(COALESCE(got, ARRAY[]::bigint[])) g
            WHERE g = ANY (COALESCE(truth, ARRAY[]::bigint[]));

            INSERT INTO bench_result
            SELECT 'nearest + where',
                   sel.label,
                   scan_setting,
                   COALESCE(array_length(got, 1), 0),
                   CASE WHEN COALESCE(array_length(truth, 1), 0) = 0 THEN NULL
                        ELSE round(overlap::numeric / array_length(truth, 1), 3) END,
                   round((SELECT percentile_cont(0.5) WITHIN GROUP (ORDER BY s)
                          FROM unnest(samples) s)::numeric, 3);
        END LOOP;
    END LOOP;
END
$bench$;

-- ---------------------------------------------------------------------------
-- The other half of the question: a threshold predicate.
--
-- `{cosine_distance: {vector: [...], threshold: 0.4}}` is a *range* over the
-- distance, and pgvector's HNSW index answers order-by-distance, not
-- distance-within-a-bound. No setting makes it index-eligible; both shapes below
-- read every row. What they separate is the cost of FraiseQL's own storage
-- contract: the emitted predicate reads the vector out of the JSONB payload and
-- parses it per row, where the same predicate against the native column does not.
-- ---------------------------------------------------------------------------
DO $threshold$
DECLARE
    runs    constant int := 3;
    qvec    vector(384);
    started timestamptz;
    samples numeric[];
    matched bigint;
    i       int;
BEGIN
    SELECT embedding INTO qvec FROM tb_doc WHERE id = 1;

    -- Shape 1: what the WHERE generator emits today — `((data->>'embedding')::vector <=> $q) <= $t`.
    samples := ARRAY[]::numeric[];
    FOR i IN 1 .. runs + 1 LOOP
        started := clock_timestamp();
        SELECT count(*) INTO matched FROM v_doc
        WHERE ((data->>'embedding')::vector <=> qvec) <= 0.4;
        IF i > 1 THEN
            samples := samples || EXTRACT(EPOCH FROM clock_timestamp() - started)::numeric * 1000;
        END IF;
    END LOOP;
    INSERT INTO bench_result VALUES ('threshold via data->>', 'n/a', 'n/a', matched, NULL,
        round((SELECT percentile_cont(0.5) WITHIN GROUP (ORDER BY s) FROM unnest(samples) s)::numeric, 3));

    -- Shape 2: the same predicate against the native column, for the difference.
    samples := ARRAY[]::numeric[];
    FOR i IN 1 .. runs + 1 LOOP
        started := clock_timestamp();
        SELECT count(*) INTO matched FROM v_doc WHERE (embedding <=> qvec) <= 0.4;
        IF i > 1 THEN
            samples := samples || EXTRACT(EPOCH FROM clock_timestamp() - started)::numeric * 1000;
        END IF;
    END LOOP;
    INSERT INTO bench_result VALUES ('threshold via native column', 'n/a', 'n/a', matched, NULL,
        round((SELECT percentile_cont(0.5) WITHIN GROUP (ORDER BY s) FROM unnest(samples) s)::numeric, 3));
END
$threshold$;

\echo
\echo '== nearest + where: k=10 over 100 000 documents, 384 dimensions =='
SELECT selectivity,
       setting,
       rows_found AS "rows",
       recall,
       ms
FROM bench_result
WHERE shape = 'nearest + where'
ORDER BY array_position(
             ARRAY['none', '50%', '5%', '1%', '0.1%', '0.01%'], selectivity),
         array_position(ARRAY['off', 'relaxed_order', 'strict_order'], setting);

\echo
\echo '== threshold predicate: no index, so what is measured is the scan =='
SELECT shape, rows_found AS "rows matched", ms
FROM bench_result
WHERE shape LIKE 'threshold%'
ORDER BY shape DESC;
