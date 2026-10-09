# PostgreSQL 18 evaluations (#1468, #1469, #1470)

PostgreSQL 18 is FraiseQL's floor (#1452). Three of its features were evaluated by
measurement before any adoption. The rigs are committed and re-runnable:

```bash
DATABASE_URL=postgresql://… tools/bench/pg18/run.sh uuidv7    1000000 3
DATABASE_URL=postgresql://… tools/bench/pg18/run.sh sql_json  100000  7
DATABASE_URL=postgresql://… tools/bench/pg18/run.sh returning 10000   7
```

Machine: Intel Core i7-13700K (24 threads), 94 GB RAM; PostgreSQL 18.6 (Debian
18.6-1.pgdg12+2, the build of CI's `ghcr.io/fraiseql/pgvector:pg18`), default configuration,
local NVMe. Measured 2026-10-09.

## #1469: `uuidv7()` as the id default — adopted for the change log

Rows inserted in committed batches of 1,000 (a write workload, not one bulk statement), on two
shipped shapes: `entity` (a Trinity table, identity primary key plus `id uuid UNIQUE`) and
`log` (`id uuid PRIMARY KEY`). Medians of 3 runs at 1M rows and 2 at 10M.

| shape | rows | generator | seconds | rows/s | WAL | uuid index |
|---|---|---|---|---|---|---|
| entity | 1M | `gen_random_uuid()` | 3.75 | 266,667 | 282 MB | 38 MB |
| entity | 1M | `uuidv7()` | 3.05 | 327,869 | 268 MB | 30 MB |
| log | 1M | `gen_random_uuid()` | 3.08 | 324,675 | 208 MB | 38 MB |
| log | 1M | `uuidv7()` | 2.36 | 423,729 | 194 MB | 30 MB |
| entity | 10M | `gen_random_uuid()` | 114.2 | 87,558 | 3,993 MB | 390 MB |
| entity | 10M | `uuidv7()` | 51.8 | 193,172 | 2,684 MB | 301 MB |
| log | 10M | `gen_random_uuid()` | 105.0 | 95,234 | 2,950 MB | 385 MB |
| log | 10M | `uuidv7()` | 41.7 | 239,894 | 1,945 MB | 301 MB |

Random v4 keys scatter inserts across the index: once it outgrows the cache (10M rows), every
insert touches a cold page and writes a full-page image. Time-ordered v7 keys insert at the
right edge: 2.2–2.5× the rate, a third less WAL and a fifth smaller index at 10M.

A v7 id carries its creation time to the millisecond and 74 random bits instead of 122.
Decided per table:

- **Adopted:** `core.tb_entity_change_log.id` (migration 08 and its CLI copy). It is the
  highest-volume table FraiseQL ships, internal, and each row already records `created_at`.
  Default-only: an existing table switches for new rows, and existing rows keep their ids.
- **Kept v4:** account, session, token, API-key, SCIM and SAML ids (an id may be shown to its
  holder, and its creation time or reduced randomness should not be); tenant and RBAC ids
  (low volume, exposed); the `fraiseql init` entity templates (public ids, the author's call).

## #1470: SQL/JSON in the generated SQL — not adopted

100k rows of a `data jsonb` entity table. Each pair returns identical rows, which the rig
checks first. Median of 7 `EXPLAIN (ANALYZE, BUFFERS)` runs.

| shape | indexed | current | SQL/JSON |
|---|---|---|---|
| projected read, `LIMIT 1000` | no | 0.785 ms | 1.375 ms (`JSON_TABLE`) |
| projected read, all rows | no | 68.8 ms | 82.0 ms (`JSON_VALUE` / `JSON_QUERY`) |
| `eq` filter | no | 5.17 ms | 9.07 ms (`JSON_VALUE`) |
| `eq` filter | expression index each | 0.008 ms | 0.003 ms (both Index Scan) |
| containment filter | no | 9.54 ms | 13.8 ms (`JSON_EXISTS`) |
| containment filter | GIN `jsonb_path_ops` | 5.64 ms (Bitmap Heap Scan) | 14.2 ms (Seq Scan) |

SQL/JSON is slower in every unindexed shape, and `JSON_EXISTS` with a filter expression
cannot use the GIN index `@>` uses. The one indexed `eq` difference is between two index scans
of a few microseconds. No shape wins, so the compiler's SQL is unchanged.

## #1468: `RETURNING OLD/NEW` — nothing for the engine to adopt

A mutation in FraiseQL is a user-written SQL function that returns its entity in
`app.mutation_response`; the engine issues no `UPDATE` and reads no after-image of its own. The
rig compares the two ways such a function can produce its response, with a `BEFORE UPDATE`
trigger rewriting `NEW`:

| body | rows | p50 | p99 | shared buffers |
|---|---|---|---|---|
| `UPDATE`, then `SELECT` the row back | 1 | 10 µs | 56 µs | 32 |
| `UPDATE … RETURNING OLD.*, NEW.*` | 1 | 8 µs | 64 µs | 7 |
| `UPDATE`, then `SELECT` the rows back | 100 | 827 µs | 1,838 µs | 1,941 |
| `UPDATE … RETURNING OLD.*, NEW.*` | 100 | 770 µs | 1,422 µs | 1,381 |

Both return the identical after-image, the trigger's rewrite included (the rig checks it).
`RETURNING` is the cheaper way to write a mutation function, and it hands back the pre-image
too. That is advice for function authors, not a change to the engine.
