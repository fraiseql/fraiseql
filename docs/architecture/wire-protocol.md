# Wire Protocol Engine (fraiseql-wire)

## What It Is

`fraiseql-wire` is a streaming JSON query engine optimized for FraiseQL's data model. It
connects to PostgreSQL and streams results as bounded-memory JSON chunks, making it
suitable for large analytical result sets where buffering the entire response is
impractical.

It is **not** a general-purpose PostgreSQL driver. It supports a narrow, purpose-built
query shape and trades generality for throughput and memory efficiency:

- **Memory**: O(chunk_size), not O(result_size) — 100-row chunks regardless of total row count
- **Throughput**: 100K–500K rows/sec, comparable to `tokio-postgres` for bulk reads
- **Time-to-first-row**: 2–5ms (results start flowing before the query completes)

## How It Differs from the Database Adapter

`fraiseql-db` and `fraiseql-wire` both talk to PostgreSQL, but they serve opposite
directions:

| | `fraiseql-db` PostgreSQL adapter | `fraiseql-wire` |
|---|---|---|
| **Direction** | FraiseQL connects *to* PostgreSQL as a client | Clients connect *to* the wire engine |
| **Protocol role** | PostgreSQL client (uses `sqlx`/`tokio-postgres`) | Streaming query engine over `deadpool-postgres` |
| **Purpose** | General query execution, mutations, introspection | Streaming large JSON result sets |
| **Query shape** | Arbitrary SQL | `SELECT data FROM {v_*|tf_*} [WHERE …] [ORDER BY …]` |
| **Result handling** | Buffer full result in memory | Stream row-by-row in bounded chunks |
| **Platform** | Cross-platform | All platforms (Unix optimized) |
| **Writes** | Full CRUD via `MutationCapable` | Read-only |

## Supported Query Shape

The wire engine accepts a narrow query form:

```sql
SELECT data
FROM v_{entity} | tv_{entity}
[WHERE predicate]
[ORDER BY expression [COLLATE collation] [ASC|DESC]]
[LIMIT N] [OFFSET M]
```

Constraints:

- Exactly one column named `data` (type `json` or `jsonb`)
- No joins, no subqueries, no mutations
- One active query per connection

## When to Use It

Use `fraiseql-wire` when:

- Streaming large result sets to clients with bounded server memory
- Integrating with tooling that expects a streaming JSON protocol
- Building pagination-free data export pipelines
- The query involves a `v_*` (view) or `tf_*` (fact table) with many rows

Use `fraiseql-db` (the standard adapter) when:

- Executing mutations
- Running schema introspection or migrations
- Running arbitrary queries (aggregations, joins)
- Memory is not a concern for the result size

## Architecture

```
Client application
      │
      │  FraiseClient::query(sql, predicates)
      ▼
fraiseql-wire
      │
      ├─ SQL predicates pushed to PostgreSQL (WHERE clause)
      ├─ Rust predicates evaluated after deserialization (hybrid filtering)
      │
      ▼
PostgreSQL (v_entity / tf_entity)
      │
      │  Rows streamed from socket incrementally
      ▼
Bounded async channel (backpressure-aware)
      │
      │  Chunks of N rows
      ▼
Consumer (async iterator)
```

Dropping the stream automatically cancels the in-flight query.

## Hybrid Filtering

The wire engine supports two-layer filtering to minimize wire traffic while allowing
application-level refinement:

```rust
// SQL predicate: pushed to the database (reduces network traffic)
let sql_predicates = vec![
    WhereClause::new("customer_id", WhereOperator::Eq, customer_id),
];

// Rust predicate: evaluated after deserialization (application logic)
let rust_predicate = |row: &Order| row.amount > 100.0;

let stream = client.query(sql, sql_predicates, Some(rust_predicate)).await?;
```

## Module Structure

| Module | Purpose |
|--------|---------|
| `auth` | Connection authentication helpers |
| `client` | `FraiseClient` — top-level entry point for queries |
| `connection` | Connection pool management (wraps `deadpool-postgres`) |
| `error` | `Error` and `Result` types |
| `json` | JSON value parsing and field extraction |
| `metrics` | `fraiseql_stream_rows_yielded`, `fraiseql_query_duration_ms`, etc. |
| `operators` | `WhereOperator` enum for SQL predicate construction |
| `protocol` | Wire-level PostgreSQL interaction |
| `stream` | Bounded async channel, backpressure, pause/resume |
| `util` | Shared utilities |

## Further Reading

- `crates/fraiseql-wire/README.md` — Usage guide and configuration reference
- `crates/fraiseql-wire/typed-streaming-guide.md` — Type-safe deserialization
- `crates/fraiseql-wire/connection-pooling.md` — Connection pool tuning
