# fraiseql-wire

**Streaming JSON queries for Postgres 17, built for FraiseQL**

`fraiseql-wire` is a **minimal, async Rust query engine** that streams JSON data from Postgres with low latency and bounded memory usage.

It is **not a general-purpose Postgres driver**.
It is a focused, purpose-built transport for queries of the form:

```sql
SELECT data
FROM v_{entity}
WHERE predicate
```

The primary goal is to enable **efficient, backpressure-aware streaming of JSON** from Postgres into Rust, leveraging Postgres 17 streaming behavior and (optionally) chunked rows mode.

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
FROM v_{entity}
WHERE <predicate>
```

### Constraints

* Exactly **one column** must be returned
* Column type must be `json` or `jsonb`
* Results are streamed in-order
* One active query per connection

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

## Project Status

⚠ **Experimental**

* API is not yet stable
* Protocol coverage is intentionally minimal
* Not recommended for production without review

That said, the design favors simplicity and auditability.

---

## Roadmap (High-Level)

* [ ] MVP: async JSON streaming via simple query protocol
* [ ] Predicate planner (SQL vs Rust)
* [ ] Cancellation support
* [ ] libpq backend with true chunked rows mode
* [ ] Typed streaming (`T: DeserializeOwned`)
* [ ] Metrics & tracing

---

## Philosophy

> *This is not a Postgres driver.*
> *It is a JSON query pipe.*

By narrowing scope, `fraiseql-wire` delivers performance and clarity that general-purpose drivers cannot.
