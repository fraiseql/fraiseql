# FraiseQL Performance

## Benchmark Suite

FraiseQL ships with a [Criterion](https://bheisler.github.io/criterion.rs/book/) benchmark suite
for regression detection. Benchmarks run automatically on every push to `dev` and every pull
request via the `bench.yml` CI workflow.

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

Regressions are reported as warnings in pull request comments but do not block merging.
Use `critcmp` locally to investigate before pushing.

## Performance Design Notes

- **Zero-cost abstractions**: `impl Trait` preferred over `Box<dyn Trait>` in hot paths.
- **Compile-time schema optimization**: SQL is generated at compile time, not at request time.
- **Connection pooling**: `deadpool-postgres` provides bounded async connection pools.
- **Single-lock LRU cache**: `QueryResultCache` uses one `Mutex<LruCache>` with atomic metric
  counters to avoid double-lock contention in the read path.
- **Arc result sharing**: Cache hits return `Arc<Vec<JsonbValue>>` — zero-copy clones.
