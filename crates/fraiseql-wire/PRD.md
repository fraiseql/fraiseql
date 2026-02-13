Below is a **Product Requirements Document (PRD)** for what you described.
It is written as if this were a real internal project (`fraiseql-wire`) and is intentionally **implementation-aware but not code-level**, so it can guide design without locking you in too early.

---

# PRD: FraiseQL-Wire

**Streaming JSON query engine for Postgres 17**

## 1. Overview

### 1.1 Purpose

FraiseQL-Wire is a **minimal, async Rust query engine** that streams JSON data from Postgres using a highly constrained SQL surface:

```sql
SELECT data
FROM v_{entity}
WHERE predicate
```

The system is designed for **low latency, low memory usage, and high throughput**, enabling consumers to process JSON results as a stream rather than materializing full result sets.

### 1.2 Non-Goals

FraiseQL-Wire is **not**:

* A general-purpose Postgres driver
* A full SQL engine
* A replacement for `tokio-postgres` or `sqlx`
* A write-capable client (no INSERT/UPDATE/DELETE)

---

## 2. Goals & Success Criteria

### 2.1 Goals

* Stream JSON results asynchronously with backpressure
* Support large result sets without high memory usage
* Leverage Postgres 17 chunked rows mode (or equivalent streaming semantics)
* Allow hybrid predicate evaluation (SQL + Rust)
* Keep implementation and API surface minimal and auditable

### 2.2 Success Criteria

* First result delivered before full query completion
* Memory usage scales with *chunk size*, not row count
* Consumer can stop iteration and cancel the query
* JSON rows are processed in order
* API feels idiomatic to async Rust users

---

## 3. Scope

### 3.1 Supported SQL Shape

Only queries of the form:

```sql
SELECT data
FROM v_{entity}
WHERE <predicate>
```

Constraints:

* Exactly **one column** returned
* Column type must be `json` or `jsonb`
* View naming convention enforced (`v_{entity}`)

### 3.2 Supported Operations

| Operation           | Support |
| ------------------- | ------- |
| SELECT              | ✅       |
| Streaming           | ✅       |
| Async               | ✅       |
| Predicate pushdown  | ✅       |
| Rust-side filtering | ✅       |
| Pagination          | ❌       |
| Writes              | ❌       |
| Transactions        | ❌       |
| Prepared statements | ❌       |

---

## 4. Architecture

### 4.1 High-Level Design

```
┌────────────────────────────┐
│ User Code                  │
│ Stream<Item = JSON>        │
└────────────▲───────────────┘
             │ backpressure
┌────────────┴───────────────┐
│ FraiseJsonStream           │
│ (Stream implementation)    │
└────────────▲───────────────┘
             │ async channel
┌────────────┴───────────────┐
│ Connection Task            │
│ - TCP socket               │
│ - Postgres protocol        │
│ - Chunked row batching     │
└────────────────────────────┘
```

### 4.2 Key Design Principles

* **Streaming first**: no full result buffering
* **One-way data flow**: server → client
* **Bounded buffers**: enforce backpressure
* **Fail fast** on unexpected schema or protocol states

---

## 5. API Design

### 5.1 Public API (Illustrative)

```rust
let stream = client
  .query("user")
  .where_predicate(predicate)
  .as_json_stream()
  .chunk_size(256)
  .execute()
  .await?;

while let Some(item) = stream.next().await {
    let json = item?;
    process(json);
}
```

### 5.2 Output Types

* Primary: `Stream<Item = Result<serde_json::Value, Error>>`
* Optional: typed stream via `DeserializeOwned`

---

## 6. Predicate Handling

### 6.1 Predicate Types

* **SQL predicates**: pushed down into SQL
* **Rust predicates**: applied to streamed JSON
* **Hybrid predicates**: SQL pre-filter + Rust refinement

### 6.2 Predicate Rules

* SQL predicate must be deterministic and side-effect free
* Rust predicates must not block
* Rust predicates must not mutate shared state

---

## 7. Streaming & Chunking

### 7.1 Chunking Model

* Rows are grouped into batches (`Vec<Bytes>`)
* Chunk size is configurable
* Chunking is transparent to consumers

### 7.2 Backpressure

* Implemented via bounded async channels
* Producer pauses when consumer lags
* Prevents unbounded memory growth

---

## 8. Protocol Support

### 8.1 Required Postgres Protocol Features

* Startup & authentication (initially trust / password)
* Simple Query protocol
* RowDescription
* DataRow
* ErrorResponse
* CommandComplete
* ReadyForQuery
* CancelRequest

### 8.2 Assumptions

* Single active query per connection
* Server returns rows in-order
* One-column result set

---

## 9. Error Handling

### 9.1 Error Categories

* Connection errors
* Authentication errors
* Protocol violations
* SQL execution errors
* JSON decoding errors
* Consumer cancellation

### 9.2 Error Propagation

* Errors are surfaced via stream items
* Fatal errors terminate the stream
* Non-fatal row-level errors may be configurable

---

## 10. Cancellation & Drop Semantics

### 10.1 Cancellation Behavior

When the stream is dropped:

* Send `CancelRequest` to Postgres
* Close socket
* Terminate connection task

### 10.2 Guarantees

* No background queries after drop
* No leaked tasks
* No orphaned connections

---

## 11. Performance Characteristics

### 11.1 Expected Benefits

* Reduced latency to first row
* Lower peak memory usage
* CPU and I/O overlap
* Efficient JSON processing

### 11.2 Tradeoffs

* Limited SQL flexibility
* Custom protocol implementation cost
* Reduced reuse of existing drivers

---

## 12. Security Considerations

* SQL injection prevented via predicate construction rules
* No dynamic SQL fragments from user input
* Authentication credentials handled out-of-band
* TLS support optional (initially)

---

## 13. Milestones

### Phase 1 — MVP

* TCP connection + simple auth
* Simple Query protocol
* Async JSON streaming
* Client-side chunking

### Phase 2 — Optimization

* Predicate pushdown planner
* Backpressure tuning
* Cancellation support

### Phase 3 — Advanced

* Postgres 17 chunked rows mode via libpq backend
* Typed streaming (`T: DeserializeOwned`)
* Metrics & tracing

---

## 14. Open Questions

* Should JSON decoding happen on producer or consumer side?
* Should `json` vs `jsonb` be enforced or configurable?
* Should batching be exposed to users or internal-only?
* Should Rust-side predicates be sandboxed?

---

## 15. Summary

FraiseQL-Wire is a **purpose-built streaming JSON query engine**, optimized for a narrow but powerful query pattern. By limiting scope, it enables:

* Simple mental model
* Predictable performance
* Clean async streaming APIs
* Efficient use of Postgres 17 capabilities

This design intentionally favors **clarity and control over generality**.
