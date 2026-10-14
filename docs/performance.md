# FraiseQL Performance

## Benchmark Suite

FraiseQL ships with a [Criterion](https://bheisler.github.io/criterion.rs/book/) benchmark suite
for regression detection. The `bench.yml` CI workflow is **dispatch-only** — no benchmark
gates a merge today, and none has since the 2026-05-31 Dagger migration removed the push
and PR triggers. See [contributing/benchmarks.md](contributing/benchmarks.md) for why, and
for how to run a comparison.

## Running Benchmarks

```bash
# Run all benchmarks
cargo bench --workspace

# Run benchmarks for a specific crate
cargo bench -p fraiseql-core
cargo bench -p fraiseql-wire

# Run a specific benchmark group
cargo bench -p fraiseql-core graphql_parse
cargo bench -p fraiseql-core cache
cargo bench -p fraiseql-wire micro_benchmarks

# Save a baseline for later comparison
cargo bench --workspace -- --save-baseline my-baseline

# Compare against saved baseline
critcmp my-baseline new-baseline
```

## Benchmark Categories

### GraphQL Parsing (`fraiseql-core/benches/graphql_parse.rs`)

Measures `parse_query` throughput across three complexity tiers:

| Benchmark | Description |
|-----------|-------------|
| `graphql_parse/query/simple` | 20-token inline query: `{ users { id name } }` |
| `graphql_parse/query/complex` | Nested query with variables and 4-level object graph |
| `graphql_parse/query/fragments` | Query with two named fragments and spread resolution |

### Cache Latency (`fraiseql-core/benches/cache.rs`)

Measures single-threaded `QueryResultCache` operation latency, isolating LRU and TTL overhead
from thread contention:

| Benchmark | Description |
|-----------|-------------|
| `cache_latency/put_hit/single` | One `put` followed by one `get` (cold → warm) |
| `cache_latency/miss/cold` | `get` on a key that was never inserted |
| `cache_latency/get/hot` | `get` on a pre-warmed 100-entry cache (always hits) |
| `cache_latency/invalidate_view/100_entries` | `invalidate_views` on a 100-entry cache |

### Cache Concurrency (`fraiseql-core/benches/cache_concurrent_bench.rs`)

Measures read, write, and mixed (90/10) throughput under 1–32 concurrent threads to demonstrate
concurrency scaling of the sharded LRU:

| Benchmark | Description |
|-----------|-------------|
| `cache_concurrent_reads` | Read throughput at 1, 4, 8, 16, 32 threads |
| `cache_concurrent_writes` | Write throughput at 1, 4, 8, 16, 32 threads |
| `cache_concurrent_mixed` | 90% reads / 10% writes at 1–32 threads |

### Wire Protocol (`fraiseql-wire/benches/micro_benchmarks.rs`)

Measures Postgres wire protocol encoding and decoding:

| Benchmark | Description |
|-----------|-------------|
| `connection_protocol` | Startup, Query, DataRow encode/decode round-trip |

### SQL Projection (`fraiseql-core/benches/sql_projection_benchmark.rs`)

Measures SQL generation throughput for various field projection patterns.

### Federation (`fraiseql-core/benches/federation_bench.rs`)

Measures Apollo Federation v2 subgraph request planning and response merging.

## Regression Thresholds

The CI workflow (`bench.yml`) applies different regression thresholds based on workload type:

| Category | Threshold | Rationale |
|----------|-----------|-----------|
| Pure computation (parser, cache, wire) | 5% | Low variance; tight threshold catches real regressions |
| DB-connected (integration, pipeline) | 15% | Higher environmental variance due to I/O |

Regressions are reported as warnings in the dispatched run's job summary; nothing blocks a
merge on them. Use `critcmp` locally to investigate before pushing.

## Performance Design Notes

- **Zero-cost abstractions**: `impl Trait` preferred over `Box<dyn Trait>` in hot paths.
- **Compile-time schema optimization**: SQL is generated at compile time, not at request time.
- **Connection pooling**: `deadpool-postgres` provides bounded async connection pools.
- **Single-lock LRU cache**: `QueryResultCache` uses one `Mutex<LruCache>` with atomic metric
  counters to avoid double-lock contention in the read path.
- **Arc result sharing**: Cache hits return `Arc<Vec<JsonbValue>>` — zero-copy clones.

## Filtering Large Documents: Use a Native Column

Reads run as prepared statements (#1557), and PostgreSQL may run a prepared statement with a
*generic plan*, one planned without the parameter values. Under that plan, an expression
index on a JSON key (`CREATE INDEX … ((data->>'status'), pk_…)`) is not used for a `where:`
on that key. The plan walks the primary key in page order and reads `data->>'status'` from
every row it rejects. When documents are large and stored out of line, every rejected row is
detoasted.

The fix is to expose the key as a column of the relation and let the query read it:

1. Add the column to the table or view (`status text NOT NULL`, kept equal to
   `data->>'status'`), with an index that ends in the page identity (`(status, pk_…)`).
2. Give the query an argument named after the column (`status`, or `customerId` for
   `customer_id`), and compile with `--database`. The compiler finds the column on the
   relation and records it in the query's `native_columns`. Without `--database` nothing is
   recorded, and the filter reads the document.

A `where:` comparison (`eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `in`, `nin`, null tests) on a
field mapped this way reads the column, if the column's type compares like the field's
(#1561). `orderBy` on that field reads the column too. Localized fields always read the
document.

Measured on PostgreSQL 18 with 7.5 KB documents stored out of line, 20 rows filtered on an
indexed `status`. Both an index on the column and an expression index on the key were
present:

| Page | Expression index on `data->>'status'` | Native `status` column |
|------|---------------------------------------|------------------------|
| offset 0 | 1,347 req/s | 5,977 req/s (4.4×) |
| offset 2000 | 15.7 req/s | 2,024 req/s (129×) |

With the column, the read is an index-only scan.

### Dates and times on a TVIEW (#1571)

A `DateTime`, `Date` or `Time` key is compared as `(data->>'key')::timestamptz` (or `::date`,
`::time`). PostgreSQL refuses to index that cast, because its result depends on the session's
`TimeZone`, so a sort or a filter on the key reads every row. On a 100 000-row TVIEW, the 20
newest rows by a document `DateTime`: a parallel sequential scan and a sort, 18 ms. By a native
`timestamptz` column with an index: an index scan, 0.02 ms, same rows.

The fix is the native column above: add it to the TVIEW's definition (pg_tviews keeps it equal
to the document's value; it adds its own `created_at` and `updated_at`, so pick another name),
index it, and map it with an argument of the field's name. `compile --database` names every
sorted or filtered temporal key a `tv_*` query reads out of its document, and says whether the
column already exists.

FraiseQL does not sort these keys as text instead. pg_tviews renders them in UTC ISO 8601
(a documented contract since beta.26), but a value it did not render (text copied through, a
row from before an upgrade) or `-infinity` would then sort silently out of order.
