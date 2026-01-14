# fraiseql-wire

[![Build Status](https://github.com/evoludigit/fraiseql-wire/workflows/CI/badge.svg?branch=main)](https://github.com/evoludigit/fraiseql-wire/actions/workflows/ci.yml)
[![Code Coverage](https://codecov.io/gh/evoludigit/fraiseql-wire/branch/main/graph/badge.svg)](https://codecov.io/gh/evoludigit/fraiseql-wire)
[![Crates.io Version](https://img.shields.io/crates/v/fraiseql-wire.svg)](https://crates.io/crates/fraiseql-wire)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/evoludigit/fraiseql-wire#license)
[![MSRV: 1.75+](https://img.shields.io/badge/MSRV-1.75%2B-blue)](https://github.com/evoludigit/fraiseql-wire)
[![Documentation](https://docs.rs/fraiseql-wire/badge.svg)](https://docs.rs/fraiseql-wire)

**Streaming JSON queries for Postgres 17, built for FraiseQL**

`fraiseql-wire` is a **minimal, async Rust query engine** that streams JSON data from Postgres with low latency and bounded memory usage.

It is **not a general-purpose Postgres driver**.
It is a focused, purpose-built transport for JSON queries of the form:

```sql
SELECT data
FROM {source}
[WHERE predicate]
[ORDER BY expression [COLLATE collation] [ASC|DESC]]
[LIMIT N] [OFFSET M]
```

Where `{source}` is a JSON-shaped relation (`v_{entity}` views or `tv_{entity}` tables).

The primary goal is to enable **efficient, backpressure-aware streaming of JSON** from Postgres into Rust, with support for hybrid filtering (SQL + Rust predicates), adaptive chunking, pause/resume flow control, and comprehensive metrics.

---

## Why fraiseql-wire?

Traditional database drivers are optimized for flexibility and completeness. FraiseQL-Wire is optimized for:

* 🚀 **Low latency** (process rows as soon as they arrive)
* 🧠 **Low memory usage** (no full result buffering)
* 🔁 **Streaming-first APIs** (`Stream<Item = Result<Value, _>>`)
* 🧩 **Hybrid filtering** (SQL + Rust predicates)
* 🔍 **JSON-native workloads**

If your application primarily:

* Reads JSON (`json` / `jsonb`)
* Uses views as an abstraction layer
* Needs to process large result sets incrementally

…then `fraiseql-wire` is a good fit.

---

## Non-goals

`fraiseql-wire` intentionally does **not** support:

* Writes (`INSERT`, `UPDATE`, `DELETE`)
* Transactions
* Prepared statements
* Arbitrary SQL
* Multi-column result sets
* Full Postgres type decoding

If you need those features, use `tokio-postgres` or `sqlx`.

---

## Supported Query Shape

All queries must conform to:

```sql
SELECT data
FROM {source}
[WHERE <predicate>]
[ORDER BY <expression> [COLLATE <collation>] [ASC|DESC]]
[LIMIT <count>]
[OFFSET <count>]
```

### Query Components

| Component | Support | Notes |
|-----------|---------|-------|
| **SELECT** | `SELECT data` only | Result column must be named `data` and type `json`/`jsonb` |
| **FROM** | `v_{entity}` / `tv_{entity}` | Views and tables with JSON column |
| **WHERE** | SQL predicates | Optional; use `where_sql()` in builder |
| **ORDER BY** | Server-side sorting | With optional COLLATE; server-executed, no client buffering |
| **LIMIT/OFFSET** | Pagination | For result set reduction |
| **Filtering** | SQL + Rust predicates | Hybrid: SQL reduces wire traffic, Rust refines streamed data |

### Hard Constraints

* Exactly **one column** in result set (named `data`)
* Column type must be `json` or `jsonb`
* Results streamed in-order (server-side ordering for ORDER BY)
* One active query per connection
* No client-side reordering or aggregation

---

## Example

### Streaming JSON results

```rust
use futures::StreamExt;

let client = FraiseClient::connect("postgres:///example").await?;

let mut stream = client
    .query("user")
    .where_sql("data->>'status' = 'active'")
    .chunk_size(256)
    .execute()
    .await?;

while let Some(item) = stream.next().await {
    let json = item?;
    println!("{json}");
}
```

### Collecting (optional)

```rust
let users: Vec<serde_json::Value> =
    stream.collect::<Result<_, _>>()?;
```

---

## Hybrid Predicates (SQL + Rust)

Not all predicates belong in SQL. FraiseQL-Wire supports **hybrid filtering**:

```rust
let stream = client
    .query("user")
    .where_sql("data->>'type' = 'customer'")
    .where_rust(|json| expensive_check(json))
    .execute()
    .await?;
```

* SQL predicates reduce data sent over the wire
* Rust predicates allow expressive, application-level filtering
* Filtering happens **while streaming**

---

## Streaming Model

Under the hood:

* Results are read incrementally from the Postgres socket
* Rows are batched into small chunks
* Chunks are sent through a bounded async channel
* Consumers apply backpressure naturally via `.await`

This ensures:

* Bounded memory usage
* CPU and I/O overlap
* Fast time-to-first-row

---

## Cancellation & Drop Semantics

If the stream is dropped early:

* The in-flight query is cancelled
* The connection is closed
* Background tasks are terminated

This prevents runaway queries and resource leaks.

---

## Postgres 17 & Chunked Rows Mode

`fraiseql-wire` is designed to take advantage of **Postgres 17 streaming behavior**, and can optionally leverage **chunked rows mode** via a libpq-based backend.

The public API remains the same regardless of backend; chunking is an internal optimization.

---

## Quick Start

### Installation

Add to `Cargo.toml`:

```toml
[dependencies]
fraiseql-wire = "0.1"
tokio = { version = "1", features = ["full"] }
futures = "0.3"
serde_json = "1"
```

### Basic Usage

```rust
use fraiseql_wire::client::FraiseClient;
use futures::stream::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to Postgres
    let client = FraiseClient::connect("postgres://localhost/mydb").await?;

    // Stream results
    let mut stream = client.query("users").execute().await?;

    while let Some(item) = stream.next().await {
        let json = item?;
        println!("{}", json);
    }

    Ok(())
}
```

### Running Examples

See `examples/` directory:

```bash
# Start Postgres with test data
docker-compose up -d

# Run examples
cargo run --example basic_query
cargo run --example filtering
cargo run --example ordering
cargo run --example streaming
cargo run --example error_handling
```

---

## Error Handling

Errors are surfaced as part of the stream:

```rust
Stream<Item = Result<serde_json::Value, FraiseError>>
```

Possible error sources include:

* Connection or authentication failures
* SQL execution errors
* Protocol violations
* Invalid result schema
* JSON decoding failures
* Query cancellation

Fatal errors terminate the stream.

For detailed error diagnosis, see [TROUBLESHOOTING.md](TROUBLESHOOTING.md).

---

## Performance Characteristics

* 📉 Memory usage scales with `chunk_size`, not result size
* ⏱ First rows are available immediately
* 🔄 Server I/O and client processing overlap
* 📦 JSON decoding is incremental

### Benchmarked Performance (v0.1.0)

**Memory Efficiency**: The key advantage

| Scenario | fraiseql-wire | tokio-postgres | Difference |
|----------|---------------|----------------|-----------|
| 10K rows | 1.3 KB | 2.6 MB | **2000x** |
| 100K rows | 1.3 KB | 26 MB | **20,000x** |
| 1M rows | 1.3 KB | 260 MB | **200,000x** |

fraiseql-wire uses **O(chunk_size)** memory while traditional drivers use **O(result_size)**.

**Latency & Throughput**: Comparable to tokio-postgres

| Metric | fraiseql-wire | tokio-postgres |
|--------|---------------|----------------|
| Connection setup | ~250 ns (CPU) | ~250 ns (CPU) |
| Query parsing | ~5-30 µs | ~5-30 µs |
| Throughput | 100K-500K rows/sec | 100K-500K rows/sec |
| Time-to-first-row | 2-5 ms | 2-5 ms |

**For detailed performance analysis**, see [PERFORMANCE_TUNING.md](PERFORMANCE_TUNING.md) and [benches/COMPARISON_GUIDE.md](benches/COMPARISON_GUIDE.md).

---

## When to Use fraiseql-wire

Use this crate if you:

* Stream large JSON result sets
* Want predictable memory usage
* Use Postgres views as an API boundary
* Prefer async streams over materialized results
* Are building FraiseQL or similar query layers

---

## When *Not* to Use It

Avoid this crate if you need:

* Writes or transactions
* Arbitrary SQL
* Strong typing across many Postgres types
* Multi-query sessions
* Compatibility with existing ORMs

---

## Advanced Features

### Type-Safe Deserialization

Stream results as custom structs instead of raw JSON:

```rust
#[derive(Deserialize)]
struct Project {
    id: String,
    name: String,
    status: String,
}

let stream = client.query::<Project>("projects").execute().await?;
while let Some(project) = stream.next().await {
    let p: Project = project?;
    println!("{}: {}", p.id, p.name);
}
```

Type `T` affects **only** deserialization; SQL, filtering, and ordering are identical regardless of `T`.

### Stream Control (Pause/Resume)

Pause and resume streams for advanced flow control:

```rust
let mut stream = client.query("entities").execute().await?;

// Process some rows
while let Some(item) = stream.next().await {
    println!("{item?}");
    break;  // Stop after one
}

// Pause to do other work
stream.pause().await?;
// ... perform other operations ...
stream.resume().await?;  // Continue from where we left off
```

### Adaptive Chunking

Automatic chunk size optimization based on channel occupancy:

```rust
let stream = client
    .query("large_table")
    .adaptive_chunking(true)    // Enabled by default
    .adaptive_min_size(16)      // Don't go below 16
    .adaptive_max_size(1024)    // Don't exceed 1024
    .execute()
    .await?;
```

### SQL Field Projection

Reduce payload size via database-level field filtering:

```rust
let stream = client
    .query("users")
    .select_projection("jsonb_build_object('id', data->>'id', 'name', data->>'name')")
    .execute()
    .await?;
// Returns only id and name fields, reducing network overhead
```

### Metrics & Tracing

Built-in metrics via the `metrics` crate:

* `fraiseql_stream_rows_yielded` – Total rows yielded from streams
* `fraiseql_stream_rows_filtered` – Rows filtered by predicates
* `fraiseql_query_duration_ms` – Query execution time
* `fraiseql_memory_usage_bytes` – Estimated memory consumption

Enable tracing with:

```bash
RUST_LOG=fraiseql_wire=debug cargo run
```

---

## Project Status

✅ **Production Ready**

* API is stable and well-tested
* 166+ unit tests, comprehensive integration tests
* Zero clippy warnings (strict `-D warnings`)
* Fully optimized streaming engine with proven performance characteristics
* Ready for production use

All core features implemented with comprehensive CI validation:
* ✅ Async JSON streaming (integration tests across PostgreSQL 15-18)
* ✅ Hybrid SQL + Rust predicates (25+ WHERE operators with full test coverage)
* ✅ Type-safe deserialization (generic streaming API with custom struct support)
* ✅ Stream pause/resume (backpressure-aware flow control)
* ✅ Adaptive chunking (automatic memory-aware chunk optimization)
* ✅ SQL field projection (SELECT clause optimization for reduced payload)
* ✅ Server-side ordering (ORDER BY with COLLATE support, no client buffering)
* ✅ Pagination (LIMIT/OFFSET for result set reduction)
* ✅ Metrics & tracing (comprehensive observability via metrics crate)
* ✅ Error handling (detailed error types and recovery patterns)
* ✅ Connection pooling support (documented integration patterns)
* ✅ TLS/SCRAM authentication (PostgreSQL 17+ security features)

---

## Roadmap

* [x] Connection pooling integration guide (CONNECTION_POOLING.md)
* [x] Advanced filtering patterns (ADVANCED_FILTERING.md)
* [x] PostgreSQL 15-18 compatibility (POSTGRES_COMPATIBILITY.md)
* [x] SCRAM/TLS end-to-end integration tests in CI
* [x] Comprehensive metrics and tracing
* [x] Server-side ordering (ORDER BY with COLLATE)
* [x] Pagination support (LIMIT/OFFSET)
* [x] SQL field projection for payload optimization
* [ ] Extended metric examples and dashboards
* [ ] Performance tuning guide for large datasets
* [ ] PostgreSQL 19+ compatibility tracking
* [ ] Binary protocol optimization (extended query protocol)

---

## Documentation & Guides

* **[QUICK_START.md](QUICK_START.md)** – Installation and first steps
* **[TESTING_GUIDE.md](TESTING_GUIDE.md)** – How to run unit, integration, and load tests
* **[TROUBLESHOOTING.md](TROUBLESHOOTING.md)** – Error diagnosis and common issues
* **[CI_CD_GUIDE.md](CI_CD_GUIDE.md)** – GitHub Actions, local development, releases
* **[PERFORMANCE_TUNING.md](PERFORMANCE_TUNING.md)** – Benchmarking and optimization
* **[CONTRIBUTING.md](CONTRIBUTING.md)** – Development workflows and architecture
* **[PRD.md](PRD.md)** – Product requirements and design
* **[.github/PUBLISHING.md](.github/PUBLISHING.md)** – Automatic crates.io publishing setup and workflow

### Examples

* **[examples/basic_query.rs](examples/basic_query.rs)** – Simple streaming usage
* **[examples/filtering.rs](examples/filtering.rs)** – SQL and Rust predicates
* **[examples/ordering.rs](examples/ordering.rs)** – ORDER BY with collation
* **[examples/streaming.rs](examples/streaming.rs)** – Large result handling and chunk tuning
* **[examples/error_handling.rs](examples/error_handling.rs)** – Error handling patterns

---

## Philosophy

> *This is not a Postgres driver.*
> *It is a JSON query pipe.*

By narrowing scope, `fraiseql-wire` delivers performance and clarity that general-purpose drivers cannot.

---

## Credits

**Author:**
- Lionel Hamayon (@evoludigit)

**Part of:** FraiseQL — Compiled GraphQL for deterministic Postgres execution

---

## License

MIT OR Apache-2.0
