# Phase 6: Polish & Documentation [QA]

## Objective

Finalize the fraiseql-wire MVP with comprehensive documentation, improved error messages, integration tests, and examples. Prepare for initial release.

## Context

This phase focuses on:
* User-facing documentation (README, examples, API docs)
* Integration tests against real Postgres
* Error message quality
* Tracing/observability
* Performance validation
* Release preparation

## Prerequisites

* Phase 5 completed (Rust predicates)

## Files to Create

* `tests/integration_full.rs` — comprehensive integration tests
* `examples/advanced_filtering.rs` — advanced filtering example
* `examples/error_handling.rs` — error handling patterns
* `CHANGELOG.md` — version history
* `CONTRIBUTING.md` — contribution guidelines

## Files to Modify

* `README.md` — expand with usage guide
* `Cargo.toml` — add package metadata for crates.io
* `src/error.rs` — improve error messages
* All modules — add comprehensive documentation

## Implementation Steps

### 1. Expand README.md

Update `/home/lionel/code/fraiseql-wire/README.md` with:

* Installation instructions
* Quick start guide
* Full API examples
* Performance characteristics
* When to use / when not to use
* Comparison to alternatives
* Troubleshooting section

(Content already exists, verify completeness)

### 2. Update Cargo.toml metadata

```toml
[package]
name = "fraiseql-wire"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
authors = ["Your Name <your.email@example.com>"]
license = "MIT OR Apache-2.0"
description = "Streaming JSON query engine for Postgres 17"
homepage = "https://github.com/yourusername/fraiseql-wire"
repository = "https://github.com/yourusername/fraiseql-wire"
documentation = "https://docs.rs/fraiseql-wire"
readme = "README.md"
keywords = ["postgres", "postgresql", "json", "streaming", "async"]
categories = ["database", "asynchronous"]

[dependencies]
tokio = { version = "1", features = ["full"] }
futures = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
bytes = "1"
thiserror = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
whoami = "1"

[dev-dependencies]
tokio-test = "0.4"
tracing-subscriber = "0.3"

# ... rest of config ...
```

### 3. Create CHANGELOG.md

```markdown
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial release (0.1.0)
- Async JSON streaming from Postgres 17
- Connection via TCP or Unix sockets
- Simple Query protocol support
- SQL predicate pushdown
- Rust-side predicate filtering
- ORDER BY support
- Configurable chunk size
- Automatic query cancellation on drop
- Bounded memory usage (scales with chunk size)
- Backpressure via async channels

### Constraints
- Single `data` column (json/jsonb)
- View naming convention: `v_{entity}`
- Read-only (no writes)
- No prepared statements
- No transaction support

## [0.1.0] - YYYY-MM-DD

Initial release.
```

### 4. Create CONTRIBUTING.md

```markdown
# Contributing to fraiseql-wire

Thank you for considering contributing to fraiseql-wire!

## Development Setup

1. Install Rust 1.75+ (via rustup)
2. Install Postgres 17
3. Clone the repository
4. Run tests: `cargo test`

## Running Integration Tests

Integration tests require a running Postgres instance:

```bash
# Start Postgres
pg_ctl start

# Run integration tests
cargo test --test integration_full -- --ignored --nocapture
```

## Code Style

* Run `cargo fmt` before committing
* Ensure `cargo clippy -- -D warnings` passes
* Add tests for new features
* Update documentation

## Pull Request Process

1. Create a feature branch
2. Implement changes with tests
3. Update CHANGELOG.md
4. Ensure all tests pass
5. Submit PR with clear description

## Design Principles

fraiseql-wire follows these principles:

* **Streaming first** — never buffer full result sets
* **Minimal scope** — narrow focus on JSON streaming
* **Fail fast** — schema violations terminate streams
* **Explicit state** — connection state machine is clear
* **No surprises** — protocol encoding/decoding is pure

## What Belongs in fraiseql-wire

✅ Features that:
* Improve JSON streaming performance
* Enhance error messages
* Add observability (metrics, tracing)
* Improve documentation

❌ Features that:
* Add general SQL support
* Implement writes/transactions
* Add connection pooling (separate crate)
* Support non-JSON types
* Require buffering full result sets

## Questions?

Open an issue for discussion before implementing large features.
```

### 5. Improve error messages in src/error.rs

Add helper methods for better error context:

```rust
impl Error {
    /// Create connection error with context
    pub fn connection<S: Into<String>>(msg: S) -> Self {
        Error::Connection(msg.into())
    }

    /// Create protocol error with context
    pub fn protocol<S: Into<String>>(msg: S) -> Self {
        Error::Protocol(msg.into())
    }

    /// Create SQL error with context
    pub fn sql<S: Into<String>>(msg: S) -> Self {
        Error::Sql(msg.into())
    }

    /// Create invalid schema error with context
    pub fn invalid_schema<S: Into<String>>(msg: S) -> Self {
        Error::InvalidSchema(msg.into())
    }

    /// Check if error is retriable
    pub fn is_retriable(&self) -> bool {
        matches!(self, Error::Io(_) | Error::ConnectionClosed)
    }

    /// Get error category for observability
    pub fn category(&self) -> &'static str {
        match self {
            Error::Connection(_) => "connection",
            Error::Authentication(_) => "authentication",
            Error::Protocol(_) => "protocol",
            Error::Sql(_) => "sql",
            Error::JsonDecode(_) => "json_decode",
            Error::Io(_) => "io",
            Error::Config(_) => "config",
            Error::Cancelled => "cancelled",
            Error::InvalidSchema(_) => "invalid_schema",
            Error::ConnectionBusy(_) => "connection_busy",
            Error::InvalidState { .. } => "invalid_state",
            Error::ConnectionClosed => "connection_closed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_category() {
        assert_eq!(Error::connection("test").category(), "connection");
        assert_eq!(Error::sql("test").category(), "sql");
    }

    #[test]
    fn test_is_retriable() {
        assert!(Error::ConnectionClosed.is_retriable());
        assert!(!Error::connection("test").is_retriable());
    }
}
```

### 6. Create tests/integration_full.rs

```rust
//! Comprehensive integration tests
//!
//! Run with: cargo test --test integration_full -- --ignored --nocapture

use fraiseql_wire::FraiseClient;
use futures::StreamExt;

#[tokio::test]
#[ignore]
async fn test_basic_connection() {
    let mut client = FraiseClient::connect("postgres://localhost/postgres")
        .await
        .expect("connect");

    client.close().await.expect("close");
}

#[tokio::test]
#[ignore]
async fn test_simple_query() {
    let mut client = FraiseClient::connect("postgres://localhost/postgres")
        .await
        .expect("connect");

    // Create temp view
    // Note: Would need to expose simple_query or use setup script

    let mut stream = client
        .query("test")
        .execute()
        .await
        .expect("query");

    let mut count = 0;
    while let Some(item) = stream.next().await {
        item.expect("item");
        count += 1;
    }

    assert!(count > 0);

    client.close().await.expect("close");
}

#[tokio::test]
#[ignore]
async fn test_sql_predicate() {
    let mut client = FraiseClient::connect("postgres://localhost/postgres")
        .await
        .expect("connect");

    let mut stream = client
        .query("test")
        .where_sql("1 = 1")
        .execute()
        .await
        .expect("query");

    while let Some(item) = stream.next().await {
        item.expect("item");
    }

    client.close().await.expect("close");
}

#[tokio::test]
#[ignore]
async fn test_rust_predicate() {
    let mut client = FraiseClient::connect("postgres://localhost/postgres")
        .await
        .expect("connect");

    let mut stream = client
        .query("test")
        .where_rust(|json| json.is_object())
        .execute()
        .await
        .expect("query");

    while let Some(item) = stream.next().await {
        let value = item.expect("item");
        assert!(value.is_object());
    }

    client.close().await.expect("close");
}

#[tokio::test]
#[ignore]
async fn test_order_by() {
    let mut client = FraiseClient::connect("postgres://localhost/postgres")
        .await
        .expect("connect");

    let mut stream = client
        .query("test")
        .order_by("data->>'id' ASC")
        .execute()
        .await
        .expect("query");

    let mut prev_id = 0i64;
    while let Some(item) = stream.next().await {
        let value = item.expect("item");
        let id = value["id"].as_i64().unwrap_or(0);
        assert!(id >= prev_id);
        prev_id = id;
    }

    client.close().await.expect("close");
}

#[tokio::test]
#[ignore]
async fn test_chunk_size() {
    let mut client = FraiseClient::connect("postgres://localhost/postgres")
        .await
        .expect("connect");

    let mut stream = client
        .query("test")
        .chunk_size(10)
        .execute()
        .await
        .expect("query");

    let mut count = 0;
    while let Some(item) = stream.next().await {
        item.expect("item");
        count += 1;
    }

    assert!(count > 0);

    client.close().await.expect("close");
}

#[tokio::test]
#[ignore]
async fn test_invalid_schema() {
    let mut client = FraiseClient::connect("postgres://localhost/postgres")
        .await
        .expect("connect");

    // Try to query a view that doesn't match the schema
    // Would need setup to create invalid view

    client.close().await.expect("close");
}

#[tokio::test]
#[ignore]
async fn test_early_drop() {
    let mut client = FraiseClient::connect("postgres://localhost/postgres")
        .await
        .expect("connect");

    let mut stream = client
        .query("test")
        .execute()
        .await
        .expect("query");

    // Read one item then drop
    if let Some(item) = stream.next().await {
        item.expect("item");
    }

    drop(stream);

    // Connection should still be usable
    // Note: Current implementation doesn't support this yet
    // Would need to track connection state properly

    client.close().await.expect("close");
}
```

### 7. Create examples/advanced_filtering.rs

```rust
//! Advanced filtering example

use fraiseql_wire::{FraiseClient, Result};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    // Uncomment to run against real database
    /*
    let mut client = FraiseClient::connect("postgres://localhost/mydb").await?;

    // Hybrid filtering: SQL + Rust
    let mut stream = client
        .query("project")
        // SQL: Reduce data over the wire
        .where_sql("data->>'status' = 'active'")
        .where_sql("(data->>'priority')::int >= 5")
        // Rust: Application-level logic
        .where_rust(|json| {
            // Complex business logic
            let estimated_cost = json["estimated_cost"].as_f64().unwrap_or(0.0);
            let team_size = json["team_size"].as_i64().unwrap_or(0);

            estimated_cost > 10_000.0 && team_size > 2
        })
        // Server-side ordering
        .order_by("data->>'name' COLLATE \"C\" ASC")
        .chunk_size(100)
        .execute()
        .await?;

    let mut count = 0;
    while let Some(item) = stream.next().await {
        let project = item?;
        println!("Project: {}", project["name"]);
        count += 1;
    }

    println!("Filtered to {} projects", count);

    client.close().await?;
    */

    println!("Example requires a database with v_project view");

    Ok(())
}
```

### 8. Create examples/error_handling.rs

```rust
//! Error handling patterns

use fraiseql_wire::{FraiseClient, Result};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    // Uncomment to run
    /*
    let connection_result = FraiseClient::connect("postgres://localhost/mydb").await;

    let mut client = match connection_result {
        Ok(client) => client,
        Err(e) => {
            eprintln!("Connection failed: {}", e);
            eprintln!("Error category: {}", e.category());

            if e.is_retriable() {
                eprintln!("This error might be retriable");
            }

            return Err(e);
        }
    };

    let stream = client.query("user").execute().await?;

    futures::pin_mut!(stream);

    while let Some(item) = stream.next().await {
        match item {
            Ok(json) => {
                println!("Row: {}", json);
            }
            Err(e) => {
                eprintln!("Row error: {}", e);
                eprintln!("Category: {}", e.category());

                // Decide whether to continue or abort
                if e.category() == "json_decode" {
                    eprintln!("Skipping invalid JSON row");
                    continue;
                } else {
                    eprintln!("Fatal error, aborting");
                    break;
                }
            }
        }
    }

    client.close().await?;
    */

    println!("Example shows error handling patterns");

    Ok(())
}
```

### 9. Add tracing spans to key operations

Update `src/connection/conn.rs`:

```rust
pub async fn startup(&mut self, config: &ConnectionConfig) -> Result<()> {
    let _span = tracing::info_span!("startup", user = %config.user, database = %config.database).entered();

    // ... existing implementation ...
}

pub async fn streaming_query(&mut self, query: &str, chunk_size: usize) -> Result<JsonStream> {
    let _span = tracing::debug_span!("streaming_query", query = %query, chunk_size = %chunk_size).entered();

    // ... existing implementation ...
}
```

Update `src/client/query_builder.rs`:

```rust
pub async fn execute(self) -> Result<impl Stream<Item = Result<Value>>> {
    let sql = self.build_sql()?;
    let _span = tracing::debug_span!("execute_query", entity = %self.entity, sql = %sql).entered();

    // ... existing implementation ...
}
```

### 10. Add module-level documentation

Ensure all modules have comprehensive module-level docs. Example for `src/protocol/mod.rs`:

```rust
//! Postgres wire protocol implementation
//!
//! This module implements the minimal subset of the Postgres wire protocol
//! needed for fraiseql-wire.
//!
//! # Supported Messages
//!
//! ## Frontend (Client → Server)
//! * Startup - initial connection
//! * Password - authentication
//! * Query - simple query protocol
//! * Terminate - close connection
//!
//! ## Backend (Server → Client)
//! * Authentication - auth requests
//! * BackendKeyData - process ID and secret key
//! * RowDescription - result schema
//! * DataRow - result row
//! * CommandComplete - query finished
//! * ReadyForQuery - ready for next query
//! * ErrorResponse - error
//! * NoticeResponse - notice
//! * ParameterStatus - session parameters
//!
//! # Not Supported
//!
//! * Extended Query protocol (prepared statements)
//! * COPY protocol
//! * Function call protocol
//! * SCRAM authentication (only cleartext for now)
//!
//! # Design
//!
//! Protocol encoding and decoding are **pure functions** (no I/O side effects).
//! All I/O happens in the connection layer.
```

## Verification Commands

```bash
# Build all targets
cargo build --all-targets

# Run unit tests
cargo test

# Run integration tests (requires Postgres)
cargo test --test integration_full -- --ignored --nocapture

# Check all examples compile
cargo check --examples

# Run examples
cargo run --example advanced_filtering
cargo run --example error_handling

# Generate documentation
cargo doc --no-deps --open

# Check documentation coverage
cargo rustdoc -- -D missing-docs

# Clippy strict mode
cargo clippy -- -D warnings -D clippy::all

# Format check
cargo fmt -- --check

# Check package
cargo package --dry-run
```

## Acceptance Criteria

- [ ] README is comprehensive and accurate
- [ ] All public APIs have documentation
- [ ] Examples demonstrate key use cases
- [ ] Integration tests cover main scenarios
- [ ] Error messages are clear and actionable
- [ ] Tracing spans are added to key operations
- [ ] CHANGELOG documents features and constraints
- [ ] CONTRIBUTING guides new contributors
- [ ] Package metadata is complete
- [ ] Documentation builds without warnings
- [ ] All tests pass
- [ ] No clippy warnings

## Performance Validation

Run a benchmark query to validate performance characteristics:

```rust
// benches/streaming_benchmark.rs (optional)
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use fraiseql_wire::FraiseClient;
use futures::StreamExt;

async fn bench_streaming() {
    let mut client = FraiseClient::connect("postgres://localhost/postgres")
        .await
        .unwrap();

    let mut stream = client
        .query("test")
        .chunk_size(256)
        .execute()
        .await
        .unwrap();

    let mut count = 0;
    while let Some(item) = stream.next().await {
        black_box(item.unwrap());
        count += 1;
    }

    client.close().await.unwrap();
}

fn criterion_benchmark(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    c.bench_function("streaming_1000_rows", |b| {
        b.to_async(&rt).iter(|| bench_streaming())
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
```

## Release Checklist

Before releasing 0.1.0:

- [ ] All phases (0-5) completed
- [ ] All tests pass
- [ ] Documentation complete
- [ ] Examples work
- [ ] CHANGELOG updated
- [ ] Version bumped in Cargo.toml
- [ ] Git tag created
- [ ] Crates.io publish (optional)

## Post-Release

After 0.1.0:

* Gather user feedback
* Identify performance bottlenecks
* Plan Phase 7 features (if needed):
  - Typed streaming (`T: DeserializeOwned`)
  - Connection pooling (separate crate?)
  - TLS support
  - SCRAM authentication
  - Metrics collection
  - Postgres 17 chunked rows mode via libpq

## Next Steps

After Phase 6, the MVP is complete. Future work depends on user feedback and real-world usage patterns.

Consider:
* Monitoring adoption and pain points
* Benchmarking against alternatives
* Identifying optimization opportunities
* Planning v0.2.0 features based on feedback
