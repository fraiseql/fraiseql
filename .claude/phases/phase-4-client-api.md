# Phase 4: Client API [QA]

## Objective

Implement the high-level public API that users interact with: connection string parsing, query builder with fluent API, and ergonomic client interface.

## Context

This phase creates the user-facing API:

```rust
let client = FraiseClient::connect("postgres://localhost/db").await?;

let mut stream = client
    .query("user")
    .where_sql("data->>'status' = 'active'")
    .chunk_size(256)
    .execute()
    .await?;

while let Some(item) = stream.next().await {
    let json = item?;
    process(json);
}
```

## Prerequisites

* Phase 3 completed (JSON streaming)

## Files to Create

* `src/client/mod.rs` — client module entry
* `src/client/client.rs` — FraiseClient type
* `src/client/query_builder.rs` — query builder API
* `src/client/connection_string.rs` — connection string parsing

## Files to Modify

* `src/lib.rs` — add `pub mod client;` and re-export `FraiseClient`

## Implementation Steps

### 1. Create src/client/connection_string.rs

```rust
//! Connection string parsing
//!
//! Supports formats:
//! * postgres://[user[:password]@][host][:port][/database]
//! * postgres:///database (Unix socket, local)

use crate::connection::ConnectionConfig;
use crate::{Error, Result};
use std::path::PathBuf;

/// Parsed connection info
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    /// Transport type
    pub transport: TransportType,
    /// Host (for TCP)
    pub host: Option<String>,
    /// Port (for TCP)
    pub port: Option<u16>,
    /// Unix socket path
    pub unix_socket: Option<PathBuf>,
    /// Database name
    pub database: String,
    /// Username
    pub user: String,
    /// Password
    pub password: Option<String>,
}

/// Transport type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportType {
    /// TCP socket
    Tcp,
    /// Unix domain socket
    Unix,
}

impl ConnectionInfo {
    /// Parse connection string
    pub fn parse(s: &str) -> Result<Self> {
        // Simple parser (production code would use url crate)
        if !s.starts_with("postgres://") && !s.starts_with("postgresql://") {
            return Err(Error::Config(
                "connection string must start with postgres://".into(),
            ));
        }

        let rest = s
            .strip_prefix("postgres://")
            .or_else(|| s.strip_prefix("postgresql://"))
            .unwrap();

        // Check if Unix socket (starts with / or no host)
        if rest.starts_with('/') || rest.starts_with("///") {
            return Self::parse_unix(rest);
        }

        Self::parse_tcp(rest)
    }

    fn parse_unix(rest: &str) -> Result<Self> {
        // Format: postgres:///database or postgres:////path/to/socket/database
        let path = rest.trim_start_matches('/');

        let database = if path.is_empty() {
            whoami::username()
        } else {
            path.to_string()
        };

        Ok(Self {
            transport: TransportType::Unix,
            host: None,
            port: None,
            unix_socket: Some(PathBuf::from("/var/run/postgresql")),
            database,
            user: whoami::username(),
            password: None,
        })
    }

    fn parse_tcp(rest: &str) -> Result<Self> {
        // Format: [user[:password]@]host[:port][/database]
        let (auth, rest) = if let Some(pos) = rest.find('@') {
            let (auth, rest) = rest.split_at(pos);
            (Some(auth), &rest[1..])
        } else {
            (None, rest)
        };

        let (user, password) = if let Some(auth) = auth {
            if let Some(pos) = auth.find(':') {
                let (user, pass) = auth.split_at(pos);
                (user.to_string(), Some(pass[1..].to_string()))
            } else {
                (auth.to_string(), None)
            }
        } else {
            (whoami::username(), None)
        };

        let (host_port, database) = if let Some(pos) = rest.find('/') {
            let (hp, db) = rest.split_at(pos);
            (hp, db[1..].to_string())
        } else {
            (rest, whoami::username())
        };

        let (host, port) = if let Some(pos) = host_port.find(':') {
            let (host, port) = host_port.split_at(pos);
            let port = port[1..]
                .parse()
                .map_err(|_| Error::Config("invalid port".into()))?;
            (host.to_string(), port)
        } else {
            (host_port.to_string(), 5432)
        };

        Ok(Self {
            transport: TransportType::Tcp,
            host: Some(host),
            port: Some(port),
            unix_socket: None,
            database,
            user,
            password,
        })
    }

    /// Convert to ConnectionConfig
    pub fn to_config(&self) -> ConnectionConfig {
        let mut config = ConnectionConfig::new(&self.database, &self.user);
        if let Some(ref password) = self.password {
            config = config.password(password);
        }
        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tcp_full() {
        let info = ConnectionInfo::parse("postgres://user:pass@localhost:5433/mydb").unwrap();
        assert_eq!(info.transport, TransportType::Tcp);
        assert_eq!(info.host, Some("localhost".to_string()));
        assert_eq!(info.port, Some(5433));
        assert_eq!(info.database, "mydb");
        assert_eq!(info.user, "user");
        assert_eq!(info.password, Some("pass".to_string()));
    }

    #[test]
    fn test_parse_tcp_minimal() {
        let info = ConnectionInfo::parse("postgres://localhost/mydb").unwrap();
        assert_eq!(info.transport, TransportType::Tcp);
        assert_eq!(info.host, Some("localhost".to_string()));
        assert_eq!(info.port, Some(5432));
        assert_eq!(info.database, "mydb");
    }

    #[test]
    fn test_parse_unix() {
        let info = ConnectionInfo::parse("postgres:///mydb").unwrap();
        assert_eq!(info.transport, TransportType::Unix);
        assert_eq!(info.database, "mydb");
    }
}
```

Add `whoami = "1"` to `Cargo.toml` dependencies.

### 2. Create src/client/query_builder.rs

```rust
//! Query builder API

use crate::stream::JsonStream;
use crate::{Error, Result};
use serde_json::Value;

/// Query builder
pub struct QueryBuilder<'a> {
    client: &'a mut crate::client::FraiseClient,
    entity: String,
    sql_predicates: Vec<String>,
    rust_predicate: Option<Box<dyn Fn(&Value) -> bool + Send>>,
    order_by: Option<String>,
    chunk_size: usize,
}

impl<'a> QueryBuilder<'a> {
    /// Create new query builder
    pub(crate) fn new(client: &'a mut crate::client::FraiseClient, entity: impl Into<String>) -> Self {
        Self {
            client,
            entity: entity.into(),
            sql_predicates: Vec::new(),
            rust_predicate: None,
            order_by: None,
            chunk_size: 256,
        }
    }

    /// Add SQL WHERE clause predicate
    ///
    /// Multiple predicates are AND'ed together
    pub fn where_sql(mut self, predicate: impl Into<String>) -> Self {
        self.sql_predicates.push(predicate.into());
        self
    }

    /// Add Rust-side predicate
    ///
    /// Applied after SQL filtering, runs on streamed JSON
    pub fn where_rust<F>(mut self, predicate: F) -> Self
    where
        F: Fn(&Value) -> bool + Send + 'static,
    {
        self.rust_predicate = Some(Box::new(predicate));
        self
    }

    /// Set ORDER BY clause
    pub fn order_by(mut self, order: impl Into<String>) -> Self {
        self.order_by = Some(order.into());
        self
    }

    /// Set chunk size (default: 256)
    pub fn chunk_size(mut self, size: usize) -> Self {
        self.chunk_size = size;
        self
    }

    /// Execute query and return JSON stream
    pub async fn execute(self) -> Result<JsonStream> {
        let sql = self.build_sql()?;
        tracing::debug!("executing query: {}", sql);

        let stream = self.client.execute_query(&sql, self.chunk_size).await?;

        // TODO: Apply rust_predicate if present (Phase 5)
        if self.rust_predicate.is_some() {
            return Err(Error::Config(
                "Rust predicates not yet implemented".into(),
            ));
        }

        Ok(stream)
    }

    /// Build SQL query
    fn build_sql(&self) -> Result<String> {
        let mut sql = format!("SELECT data FROM v_{}", self.entity);

        if !self.sql_predicates.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.sql_predicates.join(" AND "));
        }

        if let Some(ref order) = self.order_by {
            sql.push_str(" ORDER BY ");
            sql.push_str(order);
        }

        Ok(sql)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock client for testing
    struct MockClient;

    #[test]
    fn test_build_sql_simple() {
        let mut client = MockClient;
        let builder = QueryBuilder {
            client: &mut client as *mut MockClient as &mut crate::client::FraiseClient,
            entity: "user".to_string(),
            sql_predicates: vec![],
            rust_predicate: None,
            order_by: None,
            chunk_size: 256,
        };

        let sql = builder.build_sql().unwrap();
        assert_eq!(sql, "SELECT data FROM v_user");
    }

    #[test]
    fn test_build_sql_with_where() {
        let mut client = MockClient;
        let builder = QueryBuilder {
            client: &mut client as *mut MockClient as &mut crate::client::FraiseClient,
            entity: "user".to_string(),
            sql_predicates: vec!["data->>'status' = 'active'".to_string()],
            rust_predicate: None,
            order_by: None,
            chunk_size: 256,
        };

        let sql = builder.build_sql().unwrap();
        assert_eq!(sql, "SELECT data FROM v_user WHERE data->>'status' = 'active'");
    }

    #[test]
    fn test_build_sql_with_order() {
        let mut client = MockClient;
        let builder = QueryBuilder {
            client: &mut client as *mut MockClient as &mut crate::client::FraiseClient,
            entity: "user".to_string(),
            sql_predicates: vec![],
            rust_predicate: None,
            order_by: Some("data->>'name' ASC".to_string()),
            chunk_size: 256,
        };

        let sql = builder.build_sql().unwrap();
        assert_eq!(sql, "SELECT data FROM v_user ORDER BY data->>'name' ASC");
    }
}
```

### 3. Create src/client/client.rs

```rust
//! FraiseClient implementation

use super::connection_string::{ConnectionInfo, TransportType};
use super::query_builder::QueryBuilder;
use crate::connection::{Connection, Transport};
use crate::stream::JsonStream;
use crate::Result;

/// FraiseQL wire protocol client
pub struct FraiseClient {
    conn: Connection,
}

impl FraiseClient {
    /// Connect to Postgres using connection string
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> fraiseql_wire::Result<()> {
    /// use fraiseql_wire::FraiseClient;
    ///
    /// // TCP connection
    /// let client = FraiseClient::connect("postgres://localhost/mydb").await?;
    ///
    /// // Unix socket
    /// let client = FraiseClient::connect("postgres:///mydb").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn connect(connection_string: &str) -> Result<Self> {
        let info = ConnectionInfo::parse(connection_string)?;

        let transport = match info.transport {
            TransportType::Tcp => {
                let host = info.host.as_ref().expect("TCP requires host");
                let port = info.port.expect("TCP requires port");
                Transport::connect_tcp(host, port).await?
            }
            TransportType::Unix => {
                let path = info.unix_socket.as_ref().expect("Unix requires path");
                Transport::connect_unix(path).await?
            }
        };

        let mut conn = Connection::new(transport);
        let config = info.to_config();
        conn.startup(&config).await?;

        Ok(Self { conn })
    }

    /// Start building a query for an entity
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(client: &mut fraiseql_wire::FraiseClient) -> fraiseql_wire::Result<()> {
    /// let stream = client
    ///     .query("user")
    ///     .where_sql("data->>'status' = 'active'")
    ///     .order_by("data->>'name' ASC")
    ///     .execute()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn query(&mut self, entity: impl Into<String>) -> QueryBuilder {
        QueryBuilder::new(self, entity)
    }

    /// Execute a raw SQL query (must match fraiseql-wire constraints)
    pub(crate) async fn execute_query(&mut self, sql: &str, chunk_size: usize) -> Result<JsonStream> {
        self.conn.streaming_query(sql, chunk_size).await
    }

    /// Close the connection
    pub async fn close(self) -> Result<()> {
        self.conn.close().await
    }
}
```

### 4. Create src/client/mod.rs

```rust
//! High-level client API
//!
//! This module provides the user-facing API for fraiseql-wire.

mod client;
mod connection_string;
mod query_builder;

pub use client::FraiseClient;
pub use query_builder::QueryBuilder;
```

### 5. Update src/lib.rs

```rust
pub mod client;     // ADD THIS LINE
pub mod connection;
pub mod error;
pub mod json;
pub mod protocol;
pub mod stream;
pub mod util;

pub use client::FraiseClient;  // ADD THIS LINE
pub use error::{Error, Result};
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
        .where_sql("data->>'status' = 'active'")
        .order_by("data->>'name' ASC")
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

# Run client tests
cargo test client::

# Check example compiles
cargo check --example basic_stream

# Clippy
cargo clippy -- -D warnings

# Build documentation
cargo doc --no-deps --open
```

## Expected Output

### cargo test
```
running 5 tests
test client::connection_string::tests::test_parse_tcp_full ... ok
test client::connection_string::tests::test_parse_tcp_minimal ... ok
test client::connection_string::tests::test_parse_unix ... ok
test client::query_builder::tests::test_build_sql_simple ... ok
test client::query_builder::tests::test_build_sql_with_where ... ok

test result: ok. 5 passed; 0 failed; 0 ignored
```

## Acceptance Criteria

- [ ] FraiseClient can connect via connection string
- [ ] Connection string parsing handles TCP and Unix sockets
- [ ] Query builder has fluent API
- [ ] Query builder generates correct SQL
- [ ] ORDER BY is supported in query builder
- [ ] chunk_size can be configured
- [ ] Example compiles without errors
- [ ] All tests pass
- [ ] Documentation is complete
- [ ] No clippy warnings

## DO NOT

* Implement Rust predicates yet (Phase 5)
* Add typed streaming (T: DeserializeOwned) yet (Phase 6)
* Implement connection pooling (out of scope)
* Add TLS support (out of scope for MVP)
* Implement query cancellation UI (handled automatically via drop)

## Integration Test (Manual)

```rust
// tests/client_integration.rs
use fraiseql_wire::FraiseClient;
use futures::StreamExt;

#[tokio::test]
#[ignore]
async fn test_client_api() {
    let mut client = FraiseClient::connect("postgres://localhost/postgres")
        .await
        .expect("connect");

    let mut stream = client
        .query("test")
        .where_sql("1 = 1") // Dummy predicate
        .chunk_size(10)
        .execute()
        .await
        .expect("query");

    let mut count = 0;
    while let Some(item) = stream.next().await {
        item.expect("item");
        count += 1;
    }

    println!("Received {} rows", count);

    client.close().await.expect("close");
}
```

## Next Phase

**Phase 5: Rust Predicates** — Implement client-side JSON filtering with Rust predicates applied to streamed data.
