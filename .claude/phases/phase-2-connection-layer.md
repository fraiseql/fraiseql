# Phase 2: Connection Layer [GREEN]

## Objective

Implement connection management (TCP/Unix sockets) and connection state machine. This phase integrates the protocol encoding/decoding from Phase 1 with actual I/O operations to establish connections and execute simple queries.

## Context

The connection layer manages:

* Transport (TCP vs Unix socket)
* Connection lifecycle (startup, authentication, query execution)
* State machine (ensure protocol correctness)
* Message framing (send/receive complete messages)

**Key invariant**: One active query per connection.

## Prerequisites

* Phase 1 completed (protocol encoding/decoding)

## Files to Create

* `src/connection/mod.rs` — connection module entry
* `src/connection/transport.rs` — TCP/Unix socket abstraction
* `src/connection/state.rs` — connection state machine
* `src/connection/conn.rs` — main connection type

## Files to Modify

* `src/lib.rs` — add `pub mod connection;`
* `src/error.rs` — add connection-specific error variants

## Implementation Steps

### 1. Update src/error.rs (add connection errors)

```rust
/// Main error type for fraiseql-wire operations
#[derive(Debug, Error)]
pub enum Error {
    // ... existing variants ...

    /// Connection already in use
    #[error("connection busy: {0}")]
    ConnectionBusy(String),

    /// Invalid connection state
    #[error("invalid connection state: expected {expected}, got {actual}")]
    InvalidState {
        /// Expected state
        expected: String,
        /// Actual state
        actual: String,
    },

    /// Connection closed
    #[error("connection closed")]
    ConnectionClosed,
}
```

### 2. Create src/connection/transport.rs

```rust
//! Transport abstraction (TCP vs Unix socket)

use crate::Result;
use bytes::{Buf, BytesMut};
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UnixStream};

/// Transport layer abstraction
#[derive(Debug)]
pub enum Transport {
    /// TCP socket
    Tcp(TcpStream),
    /// Unix domain socket
    Unix(UnixStream),
}

impl Transport {
    /// Connect via TCP
    pub async fn connect_tcp(host: &str, port: u16) -> Result<Self> {
        let stream = TcpStream::connect((host, port)).await?;
        Ok(Transport::Tcp(stream))
    }

    /// Connect via Unix socket
    pub async fn connect_unix(path: &Path) -> Result<Self> {
        let stream = UnixStream::connect(path).await?;
        Ok(Transport::Unix(stream))
    }

    /// Write bytes to the transport
    pub async fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        match self {
            Transport::Tcp(stream) => stream.write_all(buf).await?,
            Transport::Unix(stream) => stream.write_all(buf).await?,
        }
        Ok(())
    }

    /// Flush the transport
    pub async fn flush(&mut self) -> Result<()> {
        match self {
            Transport::Tcp(stream) => stream.flush().await?,
            Transport::Unix(stream) => stream.flush().await?,
        }
        Ok(())
    }

    /// Read bytes into buffer
    pub async fn read_buf(&mut self, buf: &mut BytesMut) -> Result<usize> {
        let n = match self {
            Transport::Tcp(stream) => stream.read_buf(buf).await?,
            Transport::Unix(stream) => stream.read_buf(buf).await?,
        };
        Ok(n)
    }

    /// Shutdown the transport
    pub async fn shutdown(&mut self) -> Result<()> {
        match self {
            Transport::Tcp(stream) => stream.shutdown().await?,
            Transport::Unix(stream) => stream.shutdown().await?,
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tcp_connect_failure() {
        let result = Transport::connect_tcp("localhost", 99999).await;
        assert!(result.is_err());
    }
}
```

### 3. Create src/connection/state.rs

```rust
//! Connection state machine

use crate::{Error, Result};

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Initial state (not connected)
    Initial,

    /// Startup sent, awaiting authentication request
    AwaitingAuth,

    /// Authentication in progress
    Authenticating,

    /// Idle (ready for query)
    Idle,

    /// Query in progress
    QueryInProgress,

    /// Reading query results
    ReadingResults,

    /// Closed
    Closed,
}

impl ConnectionState {
    /// Check if transition is valid
    pub fn can_transition_to(&self, next: ConnectionState) -> bool {
        use ConnectionState::*;

        matches!(
            (self, next),
            (Initial, AwaitingAuth)
                | (AwaitingAuth, Authenticating)
                | (Authenticating, Idle)
                | (Idle, QueryInProgress)
                | (QueryInProgress, ReadingResults)
                | (ReadingResults, Idle)
                | (_, Closed)
        )
    }

    /// Transition to new state
    pub fn transition(&mut self, next: ConnectionState) -> Result<()> {
        if !self.can_transition_to(next) {
            return Err(Error::InvalidState {
                expected: format!("valid transition from {:?}", self),
                actual: format!("{:?}", next),
            });
        }
        *self = next;
        Ok(())
    }
}

impl std::fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Initial => write!(f, "initial"),
            Self::AwaitingAuth => write!(f, "awaiting_auth"),
            Self::Authenticating => write!(f, "authenticating"),
            Self::Idle => write!(f, "idle"),
            Self::QueryInProgress => write!(f, "query_in_progress"),
            Self::ReadingResults => write!(f, "reading_results"),
            Self::Closed => write!(f, "closed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_transitions() {
        let mut state = ConnectionState::Initial;
        assert!(state.transition(ConnectionState::AwaitingAuth).is_ok());
        assert!(state.transition(ConnectionState::Authenticating).is_ok());
        assert!(state.transition(ConnectionState::Idle).is_ok());
    }

    #[test]
    fn test_invalid_transition() {
        let mut state = ConnectionState::Initial;
        assert!(state.transition(ConnectionState::Idle).is_err());
    }

    #[test]
    fn test_close_from_any_state() {
        let mut state = ConnectionState::QueryInProgress;
        assert!(state.transition(ConnectionState::Closed).is_ok());
    }
}
```

### 4. Create src/connection/conn.rs

```rust
//! Core connection type

use super::state::ConnectionState;
use super::transport::Transport;
use crate::protocol::{
    decode_message, encode_message, AuthenticationMessage, BackendMessage, FrontendMessage,
};
use crate::{Error, Result};
use bytes::{Bytes, BytesMut};
use std::collections::HashMap;

/// Connection configuration
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// Database name
    pub database: String,
    /// Username
    pub user: String,
    /// Password (optional)
    pub password: Option<String>,
    /// Additional connection parameters
    pub params: HashMap<String, String>,
}

impl ConnectionConfig {
    /// Create new configuration
    pub fn new(database: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            database: database.into(),
            user: user.into(),
            password: None,
            params: HashMap::new(),
        }
    }

    /// Set password
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    /// Add connection parameter
    pub fn param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }
}

/// Postgres connection
pub struct Connection {
    transport: Transport,
    state: ConnectionState,
    read_buf: BytesMut,
    process_id: Option<i32>,
    secret_key: Option<i32>,
}

impl Connection {
    /// Create connection from transport
    pub fn new(transport: Transport) -> Self {
        Self {
            transport,
            state: ConnectionState::Initial,
            read_buf: BytesMut::with_capacity(8192),
            process_id: None,
            secret_key: None,
        }
    }

    /// Get current connection state
    pub fn state(&self) -> ConnectionState {
        self.state
    }

    /// Perform startup and authentication
    pub async fn startup(&mut self, config: &ConnectionConfig) -> Result<()> {
        self.state.transition(ConnectionState::AwaitingAuth)?;

        // Build startup parameters
        let mut params = vec![
            ("user".to_string(), config.user.clone()),
            ("database".to_string(), config.database.clone()),
        ];
        for (k, v) in &config.params {
            params.push((k.clone(), v.clone()));
        }

        // Send startup message
        let startup = FrontendMessage::Startup {
            version: crate::protocol::constants::PROTOCOL_VERSION,
            params,
        };
        self.send_message(&startup).await?;

        // Authentication loop
        self.state.transition(ConnectionState::Authenticating)?;
        self.authenticate(config).await?;

        self.state.transition(ConnectionState::Idle)?;
        Ok(())
    }

    /// Handle authentication
    async fn authenticate(&mut self, config: &ConnectionConfig) -> Result<()> {
        loop {
            let msg = self.receive_message().await?;

            match msg {
                BackendMessage::Authentication(auth) => match auth {
                    AuthenticationMessage::Ok => {
                        tracing::debug!("authentication successful");
                        break;
                    }
                    AuthenticationMessage::CleartextPassword => {
                        let password = config
                            .password
                            .as_ref()
                            .ok_or_else(|| Error::Authentication("password required".into()))?;
                        let pwd_msg = FrontendMessage::Password(password.clone());
                        self.send_message(&pwd_msg).await?;
                    }
                    AuthenticationMessage::Md5Password { .. } => {
                        return Err(Error::Authentication(
                            "MD5 authentication not yet implemented".into(),
                        ));
                    }
                },
                BackendMessage::BackendKeyData {
                    process_id,
                    secret_key,
                } => {
                    self.process_id = Some(process_id);
                    self.secret_key = Some(secret_key);
                }
                BackendMessage::ParameterStatus { name, value } => {
                    tracing::debug!("parameter status: {} = {}", name, value);
                }
                BackendMessage::ReadyForQuery { .. } => {
                    break;
                }
                BackendMessage::ErrorResponse(err) => {
                    return Err(Error::Authentication(err.to_string()));
                }
                _ => {
                    return Err(Error::Protocol(format!("unexpected message during auth: {:?}", msg)));
                }
            }
        }

        Ok(())
    }

    /// Execute a simple query (returns all backend messages)
    pub async fn simple_query(&mut self, query: &str) -> Result<Vec<BackendMessage>> {
        if self.state != ConnectionState::Idle {
            return Err(Error::ConnectionBusy(format!(
                "connection in state: {}",
                self.state
            )));
        }

        self.state.transition(ConnectionState::QueryInProgress)?;

        let query_msg = FrontendMessage::Query(query.to_string());
        self.send_message(&query_msg).await?;

        self.state.transition(ConnectionState::ReadingResults)?;

        let mut messages = Vec::new();

        loop {
            let msg = self.receive_message().await?;
            let is_ready = matches!(msg, BackendMessage::ReadyForQuery { .. });
            messages.push(msg);

            if is_ready {
                break;
            }
        }

        self.state.transition(ConnectionState::Idle)?;
        Ok(messages)
    }

    /// Send a frontend message
    async fn send_message(&mut self, msg: &FrontendMessage) -> Result<()> {
        let buf = encode_message(msg)?;
        self.transport.write_all(&buf).await?;
        self.transport.flush().await?;
        Ok(())
    }

    /// Receive a backend message
    async fn receive_message(&mut self) -> Result<BackendMessage> {
        loop {
            // Try to decode a message from buffer
            if let Ok((msg, remaining)) = decode_message(self.read_buf.clone().freeze()) {
                let consumed = self.read_buf.len() - remaining.len();
                self.read_buf.advance(consumed);
                return Ok(msg);
            }

            // Need more data
            let n = self.transport.read_buf(&mut self.read_buf).await?;
            if n == 0 {
                return Err(Error::ConnectionClosed);
            }
        }
    }

    /// Close the connection
    pub async fn close(mut self) -> Result<()> {
        self.state.transition(ConnectionState::Closed)?;
        let _ = self.send_message(&FrontendMessage::Terminate).await;
        self.transport.shutdown().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_config() {
        let config = ConnectionConfig::new("testdb", "testuser")
            .password("testpass")
            .param("application_name", "fraiseql-wire");

        assert_eq!(config.database, "testdb");
        assert_eq!(config.user, "testuser");
        assert_eq!(config.password, Some("testpass".to_string()));
        assert_eq!(
            config.params.get("application_name"),
            Some(&"fraiseql-wire".to_string())
        );
    }
}
```

### 5. Create src/connection/mod.rs

```rust
//! Connection management
//!
//! This module handles:
//! * Transport abstraction (TCP vs Unix socket)
//! * Connection lifecycle (startup, auth, query execution)
//! * State machine enforcement

mod conn;
mod state;
mod transport;

pub use conn::{Connection, ConnectionConfig};
pub use state::ConnectionState;
pub use transport::Transport;
```

### 6. Update src/lib.rs

```rust
pub mod connection;  // ADD THIS LINE
pub mod error;
pub mod protocol;
pub mod util;

pub use error::{Error, Result};
```

## Verification Commands

```bash
# Build
cargo build

# Run all tests
cargo test

# Run connection tests
cargo test connection::

# Clippy
cargo clippy -- -D warnings
```

## Expected Output

### cargo test
```
running 5 tests
test connection::state::tests::test_valid_transitions ... ok
test connection::state::tests::test_invalid_transition ... ok
test connection::state::tests::test_close_from_any_state ... ok
test connection::conn::tests::test_connection_config ... ok
test connection::transport::tests::test_tcp_connect_failure ... ok

test result: ok. 5 passed; 0 failed; 0 ignored
```

## Acceptance Criteria

- [ ] TCP and Unix socket connections work
- [ ] Connection state machine enforces valid transitions
- [ ] Startup and authentication sequence completes
- [ ] Simple query execution returns all messages
- [ ] Connection can be closed gracefully
- [ ] State transitions are validated
- [ ] All tests pass
- [ ] No clippy warnings

## DO NOT

* Implement streaming or chunking (Phase 3)
* Add connection pooling (out of scope)
* Implement TLS (out of scope for MVP)
* Add prepared statement support (not supported)
* Implement transaction support (out of scope)

## Integration Test (Manual)

Create a test file to verify against a real Postgres instance:

```bash
# Create tests/integration.rs
mkdir -p tests
```

```rust
// tests/integration.rs
use fraiseql_wire::connection::{Connection, ConnectionConfig, Transport};

#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_connect_and_query() {
    let transport = Transport::connect_tcp("localhost", 5432)
        .await
        .expect("connect");

    let mut conn = Connection::new(transport);

    let config = ConnectionConfig::new("postgres", "postgres");
    conn.startup(&config).await.expect("startup");

    let messages = conn.simple_query("SELECT 1").await.expect("query");
    assert!(!messages.is_empty());

    conn.close().await.expect("close");
}
```

Run with:
```bash
cargo test --test integration -- --ignored --nocapture
```

## Next Phase

**Phase 3: JSON Streaming** — Implement streaming abstraction, JSON row extraction, chunking, and backpressure.
