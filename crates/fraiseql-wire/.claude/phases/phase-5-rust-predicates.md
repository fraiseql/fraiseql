# Phase 5: Rust Predicates [GREENFIELD]

## Objective

Implement client-side JSON filtering using Rust predicates that are applied to the streamed JSON values. This enables hybrid filtering: SQL predicates reduce data over the wire, Rust predicates provide expressive application-level filtering.

## Context

Rust predicates allow filtering logic that:

* Cannot be expressed in SQL
* Requires application-specific logic
* Needs to access external state (read-only)
* Is more maintainable in Rust than SQL

**Design constraints**:

* Predicates must not block (async-friendly)
* Predicates must be `Send` (can cross task boundaries)
* Predicates run on each JSON value in the stream
* Failed predicates filter out the row (no error)

## Prerequisites

* Phase 4 completed (client API)

## Files to Create

* `src/stream/filter.rs` — filtered stream wrapper

## Files to Modify

* `src/stream/mod.rs` — expose filter module
* `src/client/query_builder.rs` — integrate Rust predicates

## Implementation Steps

### 1. Create src/stream/filter.rs

```rust
//! Filtered JSON stream

use crate::{Error, Result};
use futures::stream::Stream;
use serde_json::Value;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Predicate function type
pub type Predicate = Box<dyn Fn(&Value) -> bool + Send>;

/// Filtered JSON stream
pub struct FilteredStream<S> {
    inner: S,
    predicate: Predicate,
}

impl<S> FilteredStream<S> {
    /// Create new filtered stream
    pub fn new(inner: S, predicate: Predicate) -> Self {
        Self { inner, predicate }
    }
}

impl<S> Stream for FilteredStream<S>
where
    S: Stream<Item = Result<Value>> + Unpin,
{
    type Item = Result<Value>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match Pin::new(&mut self.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(value))) => {
                    // Apply predicate
                    if (self.predicate)(&value) {
                        return Poll::Ready(Some(Ok(value)));
                    }
                    // Predicate failed, try next value
                    continue;
                }
                Poll::Ready(Some(Err(e))) => {
                    // Propagate errors
                    return Poll::Ready(Some(Err(e)));
                }
                Poll::Ready(None) => {
                    // End of stream
                    return Poll::Ready(None);
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{stream, StreamExt};

    #[tokio::test]
    async fn test_filter_stream() {
        let values = vec![
            Ok(serde_json::json!({"id": 1, "active": true})),
            Ok(serde_json::json!({"id": 2, "active": false})),
            Ok(serde_json::json!({"id": 3, "active": true})),
        ];

        let inner = stream::iter(values);

        let predicate: Predicate = Box::new(|v| v["active"].as_bool().unwrap_or(false));

        let mut filtered = FilteredStream::new(inner, predicate);

        let mut results = Vec::new();
        while let Some(item) = filtered.next().await {
            let value = item.unwrap();
            results.push(value["id"].as_i64().unwrap());
        }

        assert_eq!(results, vec![1, 3]);
    }

    #[tokio::test]
    async fn test_filter_propagates_errors() {
        let values = vec![
            Ok(serde_json::json!({"id": 1})),
            Err(Error::JsonDecode(serde_json::Error::io(
                std::io::Error::new(std::io::ErrorKind::Other, "test error"),
            ))),
            Ok(serde_json::json!({"id": 2})),
        ];

        let inner = stream::iter(values);
        let predicate: Predicate = Box::new(|_| true);

        let mut filtered = FilteredStream::new(inner, predicate);

        // First item OK
        assert!(filtered.next().await.unwrap().is_ok());

        // Second item is error
        assert!(filtered.next().await.unwrap().is_err());

        // Third item OK
        assert!(filtered.next().await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn test_filter_all_filtered_out() {
        let values = vec![
            Ok(serde_json::json!({"id": 1})),
            Ok(serde_json::json!({"id": 2})),
        ];

        let inner = stream::iter(values);
        let predicate: Predicate = Box::new(|_| false); // Filter everything

        let mut filtered = FilteredStream::new(inner, predicate);

        // Stream should be empty
        assert!(filtered.next().await.is_none());
    }
}
```

### 2. Update src/stream/mod.rs

```rust
//! Streaming abstractions

mod chunking;
mod filter;     // ADD THIS LINE
mod json_stream;

pub use chunking::{ChunkingStrategy, RowChunk};
pub use filter::{FilteredStream, Predicate};  // ADD THIS LINE
pub use json_stream::{extract_json_bytes, parse_json, JsonStream};
```

### 3. Update src/client/query_builder.rs

Modify the `execute` method and add a helper to wrap the stream:

```rust
use crate::stream::{FilteredStream, JsonStream, Predicate};
use futures::stream::Stream;

impl<'a> QueryBuilder<'a> {
    // ... existing methods ...

    /// Execute query and return JSON stream
    pub async fn execute(self) -> Result<impl Stream<Item = Result<Value>>> {
        let sql = self.build_sql()?;
        tracing::debug!("executing query: {}", sql);

        let stream = self.client.execute_query(&sql, self.chunk_size).await?;

        // Apply Rust predicate if present
        if let Some(predicate) = self.rust_predicate {
            Ok(Self::wrap_filtered(stream, predicate))
        } else {
            Ok(Self::wrap_unfiltered(stream))
        }
    }

    /// Wrap stream with predicate filter
    fn wrap_filtered(
        stream: JsonStream,
        predicate: Predicate,
    ) -> impl Stream<Item = Result<Value>> {
        FilteredStream::new(stream, predicate)
    }

    /// Wrap stream without filter (helper for type unification)
    fn wrap_unfiltered(stream: JsonStream) -> impl Stream<Item = Result<Value>> {
        stream
    }

    // ... rest of implementation ...
}
```

**Note**: The return type is now `impl Stream<Item = Result<Value>>` instead of `JsonStream` to allow returning either filtered or unfiltered streams.

### 4. Update src/client/client.rs

Update the example in the docstring to show Rust predicates:

```rust
/// Start building a query for an entity
///
/// # Examples
///
/// ```no_run
/// # async fn example(client: &mut fraiseql_wire::FraiseClient) -> fraiseql_wire::Result<()> {
/// let stream = client
///     .query("user")
///     .where_sql("data->>'type' = 'customer'")  // SQL predicate
///     .where_rust(|json| {
///         // Rust predicate (applied client-side)
///         json["estimated_value"].as_f64().unwrap_or(0.0) > 1000.0
///     })
///     .order_by("data->>'name' ASC")
///     .execute()
///     .await?;
/// # Ok(())
/// # }
/// ```
pub fn query(&mut self, entity: impl Into<String>) -> QueryBuilder {
    QueryBuilder::new(self, entity)
}
```

### 5. Remove the temporary error from Phase 4

In `src/client/query_builder.rs`, remove this block from the `execute` method:

```rust
// DELETE THIS:
if self.rust_predicate.is_some() {
    return Err(Error::Config(
        "Rust predicates not yet implemented".into(),
    ));
}
```

### 6. Update examples/basic_stream.rs

```rust
//! Basic streaming example

use fraiseql_wire::{FraiseClient, Result};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    println!("fraiseql-wire v{}", fraiseql_wire::VERSION);

    // Example usage (commented out - requires Postgres)
    /*
    let mut client = FraiseClient::connect("postgres://localhost/mydb").await?;

    let mut stream = client
        .query("user")
        .where_sql("data->>'type' = 'customer'")  // Reduce data over wire
        .where_rust(|json| {
            // Application-level filtering
            json["lifetime_value"].as_f64().unwrap_or(0.0) > 10_000.0
        })
        .order_by("data->>'name' COLLATE \"C\" ASC")
        .chunk_size(256)
        .execute()
        .await?;

    let mut count = 0;
    while let Some(item) = stream.next().await {
        let json = item?;
        println!("{}", json);
        count += 1;
    }

    println!("Processed {} rows", count);

    client.close().await?;
    */

    println!("See tests/integration.rs for working examples");

    Ok(())
}
```

## Verification Commands

```bash
# Build
cargo build

# Run all tests
cargo test

# Run filter tests specifically
cargo test stream::filter

# Check example compiles
cargo check --example basic_stream

# Clippy
cargo clippy -- -D warnings
```

## Expected Output

### cargo test stream::filter

```
running 3 tests
test stream::filter::tests::test_filter_stream ... ok
test stream::filter::tests::test_filter_propagates_errors ... ok
test stream::filter::tests::test_filter_all_filtered_out ... ok

test result: ok. 3 passed; 0 failed; 0 ignored
```

## Acceptance Criteria

* [ ] FilteredStream wraps underlying stream
* [ ] Predicates filter values correctly
* [ ] Errors propagate through filtered stream
* [ ] Filtered stream implements Stream trait
* [ ] Query builder integrates Rust predicates
* [ ] Both filtered and unfiltered streams work
* [ ] Example shows hybrid filtering (SQL + Rust)
* [ ] All tests pass
* [ ] No clippy warnings

## DO NOT

* Add async predicates (predicates must be sync)
* Allow predicates to mutate state (must be read-only)
* Implement predicate composition (keep simple)
* Add predicate DSL (closures are sufficient)
* Implement parallel predicate evaluation (sequential is fine)

## Performance Considerations

* Predicates run on every streamed value
* Keep predicates lightweight (avoid expensive operations)
* Use SQL predicates for heavy filtering
* Use Rust predicates for logic that can't be expressed in SQL

## Integration Test (Manual)

```rust
// tests/rust_predicate_integration.rs
use fraiseql_wire::FraiseClient;
use futures::StreamExt;

#[tokio::test]
#[ignore]
async fn test_hybrid_filtering() {
    let mut client = FraiseClient::connect("postgres://localhost/postgres")
        .await
        .expect("connect");

    // Create test data
    let setup = r#"
        CREATE TEMP VIEW v_test AS
        SELECT json_build_object('id', i, 'value', i * 10) AS data
        FROM generate_series(1, 100) i;
    "#;

    // Execute setup (use simple_query for DDL)
    // ... (would need to expose simple_query or use separate connection)

    let mut stream = client
        .query("test")
        .where_sql("(data->>'id')::int > 50")  // SQL: filter to id > 50
        .where_rust(|json| {
            // Rust: filter to even ids
            json["id"].as_i64().unwrap_or(0) % 2 == 0
        })
        .execute()
        .await
        .expect("query");

    let mut ids = Vec::new();
    while let Some(item) = stream.next().await {
        let json = item.expect("item");
        ids.push(json["id"].as_i64().unwrap());
    }

    // Should get: 52, 54, 56, ..., 100 (25 values)
    assert_eq!(ids.len(), 25);
    assert_eq!(ids[0], 52);
    assert_eq!(ids[ids.len() - 1], 100);

    client.close().await.expect("close");
}
```

## Design Notes

**Why predicates are `Fn` not `FnMut`**:

* Predicates should not maintain state
* Makes reasoning about behavior simpler
* Allows potential parallelization later
* Enforces functional style

**Why predicates are sync not async**:

* Simpler implementation
* Encourages lightweight predicates
* Heavy operations should be in SQL
* Can revisit if real need emerges

**Type erasure trade-off**:

* Using `impl Stream` hides the concrete type
* Enables returning different stream types (filtered vs unfiltered)
* Slight loss of type information, but cleaner API

## Next Phase

**Phase 6: Polish & Documentation** — Add comprehensive examples, improve error messages, add metrics/tracing, write integration tests, and prepare for release.
