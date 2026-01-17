# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

---

## Project Overview

**fraiseql-wire** is a minimal, async Rust query engine that streams JSON data from Postgres 17.

It is **not** a general-purpose Postgres driver.

It supports exactly one constrained query family:

```sql
SELECT data
FROM {source}
WHERE predicate
[ORDER BY <expression> [COLLATE <collation>] [ASC|DESC]]
```

Where `{source}` is a JSON-shaped relation such as:

* `v_{entity}` — canonical entity views
* `tv_{entity}` — entity tables with the same JSON shape

The system is designed as a **JSON query pipe**:

* Rows are streamed in-order
* Memory usage is bounded
* No client-side reordering
* No full result buffering

---

## Design Priorities

fraiseql-wire prioritizes:

* **Low latency** — process rows as soon as they arrive
* **Bounded memory usage** — memory scales with chunk size, not result size
* **Streaming-first APIs** — `Stream<Item = Result<serde_json::Value, _>>`
* **Minimal protocol surface** — Simple Query only
* **Auditability** — from-scratch protocol implementation (no libpq)

---

## Non-Goals

fraiseql-wire explicitly does **NOT** support:

* Arbitrary SQL
* Multi-column result sets
* Client-side sorting or reordering
* Server-side cursors
* COPY protocol
* Prepared statements / Extended Query protocol
* Transactions (`BEGIN` / `COMMIT` / `ROLLBACK`)
* Write operations (`INSERT` / `UPDATE` / `DELETE`)
* Analytical SQL, including:

  * `GROUP BY`
  * `HAVING`
  * window functions (`LAG`, `LEAD`, etc.)
* Fact tables (`tf_{entity}`)
* Arrow data plane (`va_{entity}`)

If a feature requires any of the above, it **does not belong in fraiseql-wire**.
Use a general-purpose driver such as `tokio-postgres` or `sqlx` instead.

---

## Hard Invariants

These constraints are **non-negotiable** and relied upon throughout the codebase:

* Exactly **one column** in the result set
* The column must be named `data`
* The column type must be `json` or `jsonb`
* One active query per connection
* Results are streamed **in-order**
* No buffering of full result sets
* Dropping the stream cancels the query
* No client-side sorting or aggregation
* Protocol encoding/decoding is pure (no side effects)
* Connection state machine is explicit (no implicit transitions)

If a feature violates any invariant, it does **not** belong in this project.

---

## Supported Query Shape

The only supported query shape is:

```sql
SELECT data
FROM {source}
WHERE <predicate>
[ORDER BY <expression> [COLLATE <collation>] [ASC|DESC]]
```

### Ordering (`ORDER BY`)

`ORDER BY` **is supported**, with the following rules:

* Executed **entirely on the server**
* Rows are streamed in sorted order
* No client-side buffering or reordering
* Optional explicit collation is allowed

Examples:

```sql
ORDER BY project__name ASC
ORDER BY project__name COLLATE "C" DESC
```

Disallowed:

* ORDER BY combined with `GROUP BY`, `HAVING`, or window functions
* ORDER BY that would require client-side materialization

---

## Development Commands

### Build & Test

```bash
cargo build
cargo test
cargo test --test integration
cargo test test_name
cargo test -- --nocapture
```

### Linting & Formatting

```bash
cargo fmt
cargo fmt -- --check
cargo clippy
cargo clippy -- -D warnings
```

### Examples

```bash
cargo run --example basic_stream
```

---

## Architecture

### Module Structure

```
src/
├── client/         → Public API (FraiseClient, query builder)
├── stream/         → Streaming abstractions (JSON stream, chunking, cancellation)
├── protocol/       → Postgres wire protocol (encode/decode only)
├── connection/     → Connection lifecycle (TCP/Unix socket, state machine)
├── planner/        → SQL generation (WHERE + ORDER BY)
├── json/           → JSON decode & validation
├── util/           → Utilities (OIDs, bytes helpers, tracing)
└── error.rs        → Internal error types
```

### Core Architectural Principles

1. **Streaming-first** — never buffer full result sets
2. **One-way data flow** — Postgres → client only
3. **Bounded backpressure** — async channels
4. **Fail fast** — schema or protocol violations terminate the stream
5. **Single active query** — one query per connection
6. **From-scratch protocol** — no libpq, full control

---

## Connection Model

### Supported Transports

* **TCP sockets**
* **Unix domain sockets**

Both use the same Postgres wire protocol.
Transport selection is derived from the connection string:

```rust
// TCP
FraiseClient::connect("postgres://localhost:5432/db").await?;

// Unix socket
FraiseClient::connect("postgres:///db").await?;
```

---

## Query Builder API

Example:

```rust
client
    .query("project") // → SELECT data FROM v_project
    .where_sql("project__status__name = 'active'")
    .order_by("project__name COLLATE \"C\" ASC")
    .where_rust(|json| {
        json["estimated_cost"].as_f64().unwrap_or(0.0) > 10_000.0
    })
    .chunk_size(256)
    .execute()
    .await?;
```

Rules:

* SQL predicates reduce data over the wire
* Rust predicates refine the streamed data
* ORDER BY is SQL-only
* Rust predicates must not block

---

## Streaming Pipeline

```
[Postgres]
   ↓
[Socket (TCP / Unix)]
   ↓
[Protocol Decoder]
   ↓
[Chunking]
   ↓
[Async Channel] ← backpressure
   ↓
[Stream<Item = Result<Value>>]
   ↓
[User Code]
```

---

## Cancellation Semantics

When the stream is dropped:

1. Send `CancelRequest` to Postgres
2. Close the socket
3. Terminate the background task
4. Free all buffers

This prevents runaway queries and resource leaks.

---

## Protocol Implementation

Implemented **from scratch**, minimal subset only.

### Supported Messages

* Startup & authentication (trust / password)
* Simple Query
* RowDescription
* DataRow
* ErrorResponse
* CommandComplete
* ReadyForQuery
* CancelRequest

### Explicitly Not Supported

* Extended Query protocol
* Prepared statements
* COPY
* Multi-statement queries
* Transactions

---

## Database Schema Conventions

### Supported Sources

| Prefix        | Meaning                         | Support             |
| ------------- | ------------------------------- | ------------------- |
| `v_{entity}`  | Entity views (JSON data plane)  | ✅ Supported         |
| `tv_{entity}` | Entity tables (same JSON shape) | ✅ Supported         |
| `tf_{entity}` | Fact tables (analytical)        | ❌ **Not supported** |
| `va_{entity}` | Arrow data plane                | ❌ Not supported     |

### Why `tf_{entity}` Is Not Supported

Fact tables:

* Separate dimensions (`data`) from numeric measures
* Require aggregation, grouping, or window functions
* Produce derived result shapes

These characteristics violate fraiseql-wire’s invariants:

* single JSON column
* stable row semantics
* streaming without full materialization

**Analytical queries must use `tokio-postgres` or similar.**

---

## Column Naming Conventions

| Column                          | Purpose                    |
| ------------------------------- | -------------------------- |
| `id`                            | UUID v4 entity identifier  |
| `pk_*` / `fk_*`                 | Internal keys (never used) |
| `{entity}__{subentity}__id`     | Denormalized FK            |
| `{entity}__{subentity}__{attr}` | Denormalized attributes    |
| `data`                          | JSON / JSONB document      |

---

## Testing Strategy

### Unit Tests

* Protocol encode/decode
* JSON validation
* SQL generation

### Integration Tests

* End-to-end streaming
* ORDER BY correctness
* Cancellation on drop
* Error propagation
* TCP + Unix sockets
* Queries against `v_*` and `tv_*`

---

## Code Style & Conventions

* Rust edition: 2021
* `rustfmt` + strict `clippy`
* Explicit error types
* Bounded async channels only
* No blocking in async paths

---

## Performance Considerations

* Memory scales with `chunk_size`
* Time-to-first-row is critical
* Unix sockets preferred for local Postgres
* Push predicates and ORDER BY into SQL
* Avoid JSON-path predicates when possible

---

## Development Workflow

### Adding Features

1. Check against **Non-Goals**
2. Verify **Hard Invariants**
3. Update `PRD.md` if architecture changes
4. Add integration tests
5. Update README if public API changes

### Refactoring Rule

> If a module depends on both protocol details *and* async state, split it.

---

## Project Philosophy

> **This is not a Postgres driver.
> It is a JSON query pipe.**

fraiseql-wire exists to do **one thing extremely well**:
stream JSON entities from Postgres with predictable performance and a simple mental model.

Anything outside that scope belongs in another tool.
