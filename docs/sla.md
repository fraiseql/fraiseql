# Performance Characteristics

Observed behavior of FraiseQL on PostgreSQL, single-node, compiled schema, default settings.

## Latency

Measured at the server boundary (after TLS termination, before response flush), warm cache, PostgreSQL on the same network (≤1 ms RTT):

| Operation | Typical p50 | Typical p99 |
|-----------|-------------|-------------|
| Simple query (≤5 fields, no joins) | ~5 ms | ~15 ms |
| Nested query (1 level, ≤20 fields) | ~10 ms | ~30 ms |
| Mutation (single row) | ~8 ms | ~25 ms |
| Subscription delivery | ~50 ms | ~200 ms |
| Schema reload (hot) | ~500 ms | ~2 s |

These vary with hardware, schema complexity, and database load.

## Throughput

Observed on 4-core / 8 GB (release build, default connection pool):

| Workload | Observed range |
|----------|----------------|
| Simple queries (concurrent) | 5,000–30,000 req/s |
| Sequential queries (single worker) | 2,500–6,500 req/s |

The wide range reflects different query complexity and concurrency levels. Horizontal scaling is achieved by running multiple stateless server instances behind a load balancer.

## Memory

FraiseQL's compiled-schema approach keeps memory usage low. Typical baseline is 12–20 MB for the server process with default cache settings. Memory grows with cache size and concurrent connections.

## Data durability

FraiseQL does not manage data storage directly. Durability depends on the underlying database.

- All mutations execute within database transactions
- No in-memory write buffering — writes are committed before response
- Cache invalidation is best-effort; stale reads are possible within the TTL window

## Degradation behavior

When a backend dependency is unavailable:

| Dependency | Behavior |
|------------|----------|
| Database unreachable | Requests fail with 503; health endpoint reports unhealthy |
| Redis unavailable (APQ) | Falls back to in-memory APQ; no request failures |
| NATS unavailable (observers) | Observer delivery paused; queries/mutations unaffected |
| OIDC provider unreachable | Token validation fails; unauthenticated requests rejected |

## Caveats

- These are observed characteristics, not contractual guarantees. FraiseQL is open-source software provided as-is.
- Numbers apply to PostgreSQL. Other backends (MySQL, SQLite, SQL Server) may differ.
- Latency figures exclude network transit, TLS handshake, and client-side overhead.
- Subscription latency depends on the observer backend and network conditions.
- Run your own benchmarks on your hardware and schema before making capacity decisions.
