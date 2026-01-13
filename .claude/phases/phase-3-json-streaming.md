# Phase 3: JSON Streaming [REFACTOR]

## Objective

Implement async streaming abstraction that converts raw protocol messages into a stream of JSON values with backpressure, chunking, and proper cancellation semantics.

## Context

This phase builds the core streaming pipeline:

```
[Connection] → [Protocol Messages] → [Chunking] → [Channel] → [Stream<Item=Result<Value>>]
```

Key requirements:
* Bounded memory (scales with chunk size, not result size)
* Backpressure (consumer controls flow)
* Cancellation (dropping stream stops query)
* In-order delivery (no reordering)

## Prerequisites

* Phase 2 completed (connection layer)

## Files to Create

* `src/stream/mod.rs` — streaming module entry
* `src/stream/json_stream.rs` — JSON stream implementation
* `src/stream/chunking.rs` — chunking logic
* `src/json/mod.rs` — JSON validation/extraction
* `src/json/validate.rs` — result schema validation

## Files to Modify

* `src/lib.rs` — add `pub mod stream;` and `pub mod json;`
* `src/connection/conn.rs` — add streaming query method

## Implementation Steps

### 1. Create src/json/validate.rs

```rust
//! Result schema validation

use crate::protocol::{BackendMessage, FieldDescription};
use crate::util::oid::is_json_oid;
use crate::{Error, Result};

/// Validate that RowDescription matches our requirements
pub fn validate_row_description(msg: &BackendMessage) -> Result<()> {
    let fields = match msg {
        BackendMessage::RowDescription(fields) => fields,
        _ => return Err(Error::Protocol("expected RowDescription".into())),
    };

    // Must have exactly one column
    if fields.len() != 1 {
        return Err(Error::InvalidSchema(format!(
            "expected 1 column, got {}",
            fields.len()
        )));
    }

    let field = &fields[0];

    // Column must be named "data"
    if field.name != "data" {
        return Err(Error::InvalidSchema(format!(
            "expected column named 'data', got '{}'",
            field.name
        )));
    }

    // Type must be json or jsonb
    if !is_json_oid(field.type_oid) {
        return Err(Error::InvalidSchema(format!(
            "expected json/jsonb type, got OID {}",
            field.type_oid
        )));
    }

    Ok(())
}

/// Extract field description from RowDescription
pub fn extract_field_description(msg: &BackendMessage) -> Result<FieldDescription> {
    let fields = match msg {
        BackendMessage::RowDescription(fields) => fields,
        _ => return Err(Error::Protocol("expected RowDescription".into())),
    };

    Ok(fields[0].clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::oid::JsonOid;

    #[test]
    fn test_valid_row_description() {
        let field = FieldDescription {
            name: "data".to_string(),
            table_oid: 0,
            column_attr: 0,
            type_oid: JsonOid,
            type_size: -1,
            type_modifier: -1,
            format_code: 0,
        };

        let msg = BackendMessage::RowDescription(vec![field]);
        assert!(validate_row_description(&msg).is_ok());
    }

    #[test]
    fn test_wrong_column_name() {
        let field = FieldDescription {
            name: "wrong".to_string(),
            table_oid: 0,
            column_attr: 0,
            type_oid: JsonOid,
            type_size: -1,
            type_modifier: -1,
            format_code: 0,
        };

        let msg = BackendMessage::RowDescription(vec![field]);
        assert!(validate_row_description(&msg).is_err());
    }

    #[test]
    fn test_wrong_type() {
        let field = FieldDescription {
            name: "data".to_string(),
            table_oid: 0,
            column_attr: 0,
            type_oid: 23, // INT4
            type_size: 4,
            type_modifier: -1,
            format_code: 0,
        };

        let msg = BackendMessage::RowDescription(vec![field]);
        assert!(validate_row_description(&msg).is_err());
    }

    #[test]
    fn test_multiple_columns() {
        let field1 = FieldDescription {
            name: "data".to_string(),
            table_oid: 0,
            column_attr: 0,
            type_oid: JsonOid,
            type_size: -1,
            type_modifier: -1,
            format_code: 0,
        };
        let field2 = field1.clone();

        let msg = BackendMessage::RowDescription(vec![field1, field2]);
        assert!(validate_row_description(&msg).is_err());
    }
}
```

### 2. Create src/json/mod.rs

```rust
//! JSON handling and validation

mod validate;

pub use validate::{extract_field_description, validate_row_description};
```

### 3. Create src/stream/chunking.rs

```rust
//! Chunking logic for batching rows

use bytes::Bytes;

/// Row chunk (batch of raw JSON bytes)
pub struct RowChunk {
    rows: Vec<Bytes>,
}

impl RowChunk {
    /// Create new chunk
    pub fn new() -> Self {
        Self { rows: Vec::new() }
    }

    /// Create with capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            rows: Vec::with_capacity(capacity),
        }
    }

    /// Add row to chunk
    pub fn push(&mut self, row: Bytes) {
        self.rows.push(row);
    }

    /// Check if chunk is empty
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Get chunk size
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Consume chunk and return rows
    pub fn into_rows(self) -> Vec<Bytes> {
        self.rows
    }
}

impl Default for RowChunk {
    fn default() -> Self {
        Self::new()
    }
}

/// Chunking strategy
pub struct ChunkingStrategy {
    chunk_size: usize,
}

impl ChunkingStrategy {
    /// Create new strategy with given chunk size
    pub fn new(chunk_size: usize) -> Self {
        Self { chunk_size }
    }

    /// Check if chunk is full
    pub fn is_full(&self, chunk: &RowChunk) -> bool {
        chunk.len() >= self.chunk_size
    }

    /// Create new chunk with appropriate capacity
    pub fn new_chunk(&self) -> RowChunk {
        RowChunk::with_capacity(self.chunk_size)
    }
}

impl Default for ChunkingStrategy {
    fn default() -> Self {
        Self::new(256)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_operations() {
        let mut chunk = RowChunk::new();
        assert!(chunk.is_empty());

        chunk.push(Bytes::from_static(b"{}"));
        assert_eq!(chunk.len(), 1);
        assert!(!chunk.is_empty());
    }

    #[test]
    fn test_chunking_strategy() {
        let strategy = ChunkingStrategy::new(2);
        let mut chunk = strategy.new_chunk();

        assert!(!strategy.is_full(&chunk));

        chunk.push(Bytes::from_static(b"{}"));
        assert!(!strategy.is_full(&chunk));

        chunk.push(Bytes::from_static(b"{}"));
        assert!(strategy.is_full(&chunk));
    }
}
```

### 4. Create src/stream/json_stream.rs

```rust
//! JSON stream implementation

use crate::protocol::BackendMessage;
use crate::{Error, Result};
use bytes::Bytes;
use futures::stream::Stream;
use serde_json::Value;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;

/// JSON value stream
pub struct JsonStream {
    receiver: mpsc::Receiver<Result<Value>>,
    _cancel_tx: mpsc::Sender<()>, // Dropped when stream is dropped
}

impl JsonStream {
    /// Create new JSON stream
    pub(crate) fn new(
        receiver: mpsc::Receiver<Result<Value>>,
        cancel_tx: mpsc::Sender<()>,
    ) -> Self {
        Self {
            receiver,
            _cancel_tx: cancel_tx,
        }
    }
}

impl Stream for JsonStream {
    type Item = Result<Value>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}

/// Extract JSON bytes from DataRow message
pub fn extract_json_bytes(msg: &BackendMessage) -> Result<Bytes> {
    match msg {
        BackendMessage::DataRow(fields) => {
            if fields.len() != 1 {
                return Err(Error::Protocol(format!(
                    "expected 1 field, got {}",
                    fields.len()
                )));
            }

            let field = &fields[0];
            field
                .clone()
                .ok_or_else(|| Error::Protocol("null data field".into()))
        }
        _ => Err(Error::Protocol("expected DataRow".into())),
    }
}

/// Parse JSON bytes into Value
pub fn parse_json(data: Bytes) -> Result<Value> {
    let value: Value = serde_json::from_slice(&data)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_bytes() {
        let data = Bytes::from_static(b"{\"key\":\"value\"}");
        let msg = BackendMessage::DataRow(vec![Some(data.clone())]);

        let extracted = extract_json_bytes(&msg).unwrap();
        assert_eq!(extracted, data);
    }

    #[test]
    fn test_extract_null_field() {
        let msg = BackendMessage::DataRow(vec![None]);
        assert!(extract_json_bytes(&msg).is_err());
    }

    #[test]
    fn test_parse_json() {
        let data = Bytes::from_static(b"{\"key\":\"value\"}");
        let value = parse_json(data).unwrap();

        assert_eq!(value["key"], "value");
    }

    #[test]
    fn test_parse_invalid_json() {
        let data = Bytes::from_static(b"not json");
        assert!(parse_json(data).is_err());
    }
}
```

### 5. Create src/stream/mod.rs

```rust
//! Streaming abstractions

mod chunking;
mod json_stream;

pub use chunking::{ChunkingStrategy, RowChunk};
pub use json_stream::{extract_json_bytes, parse_json, JsonStream};
```

### 6. Update src/connection/conn.rs (add streaming query)

Add this method to the `Connection` impl:

```rust
use crate::json::validate_row_description;
use crate::stream::{extract_json_bytes, parse_json, ChunkingStrategy, JsonStream, RowChunk};
use tokio::sync::mpsc;

impl Connection {
    // ... existing methods ...

    /// Execute a streaming query
    pub async fn streaming_query(
        &mut self,
        query: &str,
        chunk_size: usize,
    ) -> Result<JsonStream> {
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

        // Read RowDescription first
        let row_desc = self.receive_message().await?;
        validate_row_description(&row_desc)?;

        // Create channels
        let (result_tx, result_rx) = mpsc::channel(chunk_size);
        let (cancel_tx, mut cancel_rx) = mpsc::channel::<()>(1);

        // Spawn background task to read rows
        let mut conn_buf = self.read_buf.clone();
        let mut transport = std::mem::replace(
            &mut self.transport,
            Transport::connect_tcp("dummy", 0).await?, // Placeholder
        );

        tokio::spawn(async move {
            let strategy = ChunkingStrategy::new(chunk_size);
            let mut chunk = strategy.new_chunk();

            loop {
                tokio::select! {
                    // Check for cancellation
                    _ = cancel_rx.recv() => {
                        tracing::debug!("query cancelled");
                        break;
                    }

                    // Read next message
                    msg_result = Self::receive_message_static(&mut transport, &mut conn_buf) => {
                        match msg_result {
                            Ok(msg) => match msg {
                                BackendMessage::DataRow(_) => {
                                    match extract_json_bytes(&msg) {
                                        Ok(json_bytes) => {
                                            chunk.push(json_bytes);

                                            if strategy.is_full(&chunk) {
                                                if let Err(_) = Self::send_chunk(&result_tx, chunk).await {
                                                    break; // Receiver dropped
                                                }
                                                chunk = strategy.new_chunk();
                                            }
                                        }
                                        Err(e) => {
                                            let _ = result_tx.send(Err(e)).await;
                                            break;
                                        }
                                    }
                                }
                                BackendMessage::CommandComplete(_) => {
                                    // Send remaining chunk
                                    if !chunk.is_empty() {
                                        let _ = Self::send_chunk(&result_tx, chunk).await;
                                    }
                                }
                                BackendMessage::ReadyForQuery { .. } => {
                                    break;
                                }
                                BackendMessage::ErrorResponse(err) => {
                                    let _ = result_tx.send(Err(Error::Sql(err.to_string()))).await;
                                    break;
                                }
                                _ => {
                                    let _ = result_tx.send(Err(Error::Protocol(
                                        format!("unexpected message: {:?}", msg)
                                    ))).await;
                                    break;
                                }
                            },
                            Err(e) => {
                                let _ = result_tx.send(Err(e)).await;
                                break;
                            }
                        }
                    }
                }
            }
        });

        Ok(JsonStream::new(result_rx, cancel_tx))
    }

    /// Helper: receive message (static version for spawned task)
    async fn receive_message_static(
        transport: &mut Transport,
        buf: &mut bytes::BytesMut,
    ) -> Result<BackendMessage> {
        use bytes::Buf;

        loop {
            if let Ok((msg, remaining)) = crate::protocol::decode_message(buf.clone().freeze()) {
                let consumed = buf.len() - remaining.len();
                buf.advance(consumed);
                return Ok(msg);
            }

            let n = transport.read_buf(buf).await?;
            if n == 0 {
                return Err(Error::ConnectionClosed);
            }
        }
    }

    /// Helper: send chunk to channel
    async fn send_chunk(
        tx: &mpsc::Sender<Result<Value>>,
        chunk: RowChunk,
    ) -> std::result::Result<(), ()> {
        for row_bytes in chunk.into_rows() {
            match parse_json(row_bytes) {
                Ok(value) => {
                    if tx.send(Ok(value)).await.is_err() {
                        return Err(()); // Receiver dropped
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                    return Err(());
                }
            }
        }
        Ok(())
    }
}
```

### 7. Update src/lib.rs

```rust
pub mod connection;
pub mod error;
pub mod json;      // ADD THIS LINE
pub mod protocol;
pub mod stream;    // ADD THIS LINE
pub mod util;

pub use error::{Error, Result};
```

## Verification Commands

```bash
# Build
cargo build

# Run all tests
cargo test

# Run streaming tests
cargo test stream::
cargo test json::

# Clippy
cargo clippy -- -D warnings
```

## Expected Output

### cargo test
```
running 10 tests
test json::validate::tests::test_valid_row_description ... ok
test json::validate::tests::test_wrong_column_name ... ok
test json::validate::tests::test_wrong_type ... ok
test json::validate::tests::test_multiple_columns ... ok
test stream::chunking::tests::test_chunk_operations ... ok
test stream::chunking::tests::test_chunking_strategy ... ok
test stream::json_stream::tests::test_extract_json_bytes ... ok
test stream::json_stream::tests::test_extract_null_field ... ok
test stream::json_stream::tests::test_parse_json ... ok
test stream::json_stream::tests::test_parse_invalid_json ... ok

test result: ok. 10 passed; 0 failed; 0 ignored
```

## Acceptance Criteria

- [ ] JSON stream implements `Stream<Item = Result<Value>>`
- [ ] Row schema validation enforces single `data` column
- [ ] Row schema validation enforces JSON/JSONB type
- [ ] Chunking batches rows efficiently
- [ ] Backpressure works (bounded channel)
- [ ] Cancellation works (dropping stream stops background task)
- [ ] Invalid JSON returns error in stream
- [ ] All tests pass
- [ ] No clippy warnings

## DO NOT

* Implement client API yet (Phase 4)
* Add query builder (Phase 4)
* Implement Rust-side predicates yet (Phase 5)
* Add typed streaming (T: DeserializeOwned) yet (Phase 6)
* Implement connection pooling (out of scope)

## Integration Test (Manual)

```rust
// tests/streaming_integration.rs
use fraiseql_wire::connection::{Connection, ConnectionConfig, Transport};
use futures::StreamExt;

#[tokio::test]
#[ignore]
async fn test_streaming_query() {
    let transport = Transport::connect_tcp("localhost", 5432)
        .await
        .expect("connect");

    let mut conn = Connection::new(transport);

    let config = ConnectionConfig::new("postgres", "postgres");
    conn.startup(&config).await.expect("startup");

    // Assumes you have a view with JSON data
    let mut stream = conn
        .streaming_query("SELECT '{\"key\": \"value\"}'::json AS data", 10)
        .await
        .expect("query");

    let mut count = 0;
    while let Some(item) = stream.next().await {
        let value = item.expect("value");
        assert_eq!(value["key"], "value");
        count += 1;
    }

    assert_eq!(count, 1);
}
```

## Next Phase

**Phase 4: Client API** — High-level public API with query builder, connection string parsing, and ergonomic streaming interface.
