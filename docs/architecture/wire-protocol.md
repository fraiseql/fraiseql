# Wire Protocol Engine (fraiseql-wire)

## What It Is

`fraiseql-wire` is a streaming JSON query client for PostgreSQL. It implements the
PostgreSQL frontend/backend protocol itself (startup, SCRAM authentication, TLS, simple
query), connects to PostgreSQL over TCP or a Unix socket, and streams the `data` column of
each row as JSON in bounded chunks.

It is **not** a general-purpose PostgreSQL driver. It supports one narrow query shape:

- **Memory**: rows are buffered per chunk, not per result. The default chunk size is 256
  rows (`QueryBuilder::chunk_size`); adaptive chunking and memory soft limits are opt-in.
- **Platform**: Unix-like systems only. The crate fails to compile elsewhere
  (`compile_error!` in `src/lib.rs`).

## How It Differs from the Database Adapter

`fraiseql-db` and `fraiseql-wire` are both PostgreSQL clients. They differ in what they
can run:

| | `fraiseql-db` PostgreSQL adapter | `fraiseql-wire` |
|---|---|---|
| **Protocol role** | PostgreSQL client (`tokio-postgres` with a `deadpool-postgres` pool) | PostgreSQL client with its own protocol implementation; one connection per `FraiseClient`, no pool |
| **Purpose** | Query execution, mutations, introspection | Streaming large JSON result sets |
| **Query shape** | Any SQL the engine builds | `SELECT data FROM {entity} [WHERE …] [ORDER BY …] [LIMIT N] [OFFSET M]` |
| **Platform** | Cross-platform | Unix-like systems only |
| **Writes** | Yes, through the `Writer` trait | Read-only |

`fraiseql-db` wraps this crate as `FraiseWireAdapter` behind its `wire-backend` feature.
That adapter is not a `Writer`, so an embedder serves it with `Server::new_read_only`.
The `fraiseql-server` binary does not use it: it boots on `PostgresAdapter` only (#1566).

## Supported Query Shape

The query builder (`FraiseClient::query::<T>(entity)`) emits this form:

```sql
SELECT data            -- or SELECT <projection> AS data, via select_projection()
FROM {entity}
[WHERE predicate [AND predicate ...]]
[ORDER BY expression]
[LIMIT N] [OFFSET M]
```

Constraints:

- One column named `data` (type `json` or `jsonb`)
- No joins, no subqueries, no mutations
- One active query per connection

The type parameter `T` only controls how each row is deserialized on the consumer side. It
does not change the SQL.

## When to Use It

Use `fraiseql-wire` when:

- Streaming large result sets with bounded client memory
- Building data export pipelines that read one view row by row

Use `fraiseql-db` (the standard adapter) when:

- Executing mutations
- Running schema introspection or migrations
- Running any query outside the shape above (aggregations, joins)

## Architecture

```
Client application
      │
      │  FraiseClient::query::<T>(entity).where_sql(..).where_rust(..).execute()
      ▼
fraiseql-wire
      │
      ├─ SQL predicates sent to PostgreSQL (WHERE clause)
      ├─ Rust predicate evaluated on each JSON row (hybrid filtering)
      │
      ▼
PostgreSQL (the named view or table)
      │
      │  Rows read from the socket incrementally
      ▼
Bounded async channel (backpressure, pause/resume)
      │
      │  Chunks of N rows
      ▼
Consumer (async stream of T)
```

Dropping the stream stops the background reader task. The crate does not send a
PostgreSQL cancel request.

## Hybrid Filtering

The query builder supports two-layer filtering: SQL predicates reduce what PostgreSQL
sends, and a Rust predicate refines rows after they arrive.

```rust
let stream = client
    .query::<Order>("v_order")
    // SQL predicate: added to the WHERE clause, AND-ed with any others
    .where_sql("data->>'customer_id' = 'c-42'")
    // Rust predicate: evaluated on each row's JSON value before deserialization into T
    .where_rust(|row| row["amount"].as_f64().is_some_and(|a| a > 100.0))
    .execute()
    .await?;
```

## Module Structure

| Module | Purpose |
|--------|---------|
| `auth` | SCRAM-SHA-256 authentication |
| `client` | `FraiseClient`, connection strings, and the `QueryBuilder` |
| `connection` | Transport (TCP or Unix socket), TLS, connection state machine |
| `error` | `WireError` and `Result` types |
| `json` | JSON validation of row payloads |
| `metrics` | Counters, gauges and histograms, e.g. `fraiseql_queries_total`, `fraiseql_query_total_duration_ms`, `fraiseql_rows_processed_total` |
| `operators` | `WhereOperator`, `Field`, `OrderByClause` for SQL predicate construction |
| `protocol` | PostgreSQL message encoding and decoding |
| `stream` | Chunking, adaptive chunking, filtering, typed streams, pause/resume |
| `util` | Byte and OID helpers |

## Further Reading

- `crates/fraiseql-wire/README.md` — Usage guide and configuration reference
- `crates/fraiseql-wire/typed-streaming-guide.md` — Type-safe deserialization
- `crates/fraiseql-wire/connection-pooling.md` — Using the client with an external pool
