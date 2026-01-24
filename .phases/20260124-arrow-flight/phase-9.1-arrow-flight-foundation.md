# Phase 9.1: Arrow Flight Foundation

**Duration**: 5-7 days
**Priority**: ⭐⭐⭐⭐⭐ (Foundation for entire Phase 9)
**Status**: Ready to implement

---

## Objective

Establish the foundational Arrow Flight infrastructure for FraiseQL, including:
- New `fraiseql-arrow` crate with Flight server trait
- gRPC server lifecycle management
- Core Flight RPC methods (DoGet, DoPut, GetSchema, ListFlights)
- Basic schema definitions and transmission
- Integration tests for server lifecycle

This phase creates the **skeleton** that subsequent phases will build upon.

---

## Context

Apache Arrow Flight is a gRPC-based protocol for high-performance columnar data transfer. It consists of:

1. **Flight RPC Methods**:
   - `DoGet(ticket)` - Fetch data stream from server
   - `DoPut(stream)` - Upload data stream to server
   - `GetSchema(descriptor)` - Get Arrow schema without data
   - `ListFlights(criteria)` - List available datasets/queries

2. **Arrow Data Format**:
   - `Schema` - Column names + types
   - `RecordBatch` - Columnar data chunk (like a DataFrame)
   - Zero-copy deserialization in client

3. **Integration with FraiseQL**:
   - `fraiseql-server` will host both HTTP (GraphQL) and gRPC (Arrow Flight)
   - `fraiseql-core` will gain capability to output Arrow RecordBatches
   - Clients can choose transport: HTTP/JSON (traditional) or Arrow Flight (analytics)

**Why a new crate?**
- Arrow Flight has significant dependencies (arrow, tonic, prost)
- Optional feature for users who don't need high-performance analytics
- Clean separation of concerns

---

## Files to Create

### 1. New Crate: `crates/fraiseql-arrow/`

```
crates/fraiseql-arrow/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── flight_server.rs       # Flight service trait + implementation
│   ├── schema.rs               # Arrow schema definitions
│   ├── ticket.rs               # Flight ticket encoding/decoding
│   └── error.rs                # Arrow-specific errors
└── tests/
    └── integration_test.rs     # Server lifecycle tests
```

### 2. Workspace Configuration

**File**: `Cargo.toml` (workspace root)
- Add `fraiseql-arrow` to workspace members

### 3. Server Integration

**File**: `crates/fraiseql-server/Cargo.toml`
- Add optional `arrow` feature
- Add `fraiseql-arrow` as optional dependency

---

## Files to Modify

### 1. `Cargo.toml` (workspace root)

Add new crate to workspace:

```toml
[workspace]
members = [
    "crates/fraiseql-core",
    "crates/fraiseql-server",
    "crates/fraiseql-cli",
    "crates/fraiseql-wire",
    "crates/fraiseql-observers",
    "crates/fraiseql-arrow",  # NEW
]
```

### 2. `crates/fraiseql-server/Cargo.toml`

Add optional Arrow Flight support:

```toml
[features]
default = ["http"]
http = []
arrow = ["fraiseql-arrow"]  # NEW

[dependencies]
fraiseql-arrow = { path = "../fraiseql-arrow", optional = true }  # NEW
```

---

## Implementation Steps

### Step 1: Create `fraiseql-arrow` Crate (30 min)

**File**: `crates/fraiseql-arrow/Cargo.toml`

```toml
[package]
name = "fraiseql-arrow"
version = "0.1.0"
edition = "2021"

[dependencies]
# Arrow ecosystem
arrow = "53"
arrow-flight = "53"
arrow-schema = "53"

# gRPC framework
tonic = "0.12"
prost = "0.13"

# Async runtime
tokio = { version = "1", features = ["full"] }
tokio-stream = "0.1"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Error handling
thiserror = "2"

# Logging
tracing = "0.1"

[dev-dependencies]
tokio-test = "0.4"
```

**Verification**:
```bash
cd crates/fraiseql-arrow
cargo check
# Should compile with no errors
```

---

### Step 2: Define Arrow-Specific Errors (30 min)

**File**: `crates/fraiseql-arrow/src/error.rs`

```rust
use thiserror::Error;

/// Errors specific to Arrow Flight operations.
#[derive(Debug, Error)]
pub enum ArrowFlightError {
    #[error("Arrow error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),

    #[error("Flight error: {0}")]
    Flight(String),

    #[error("Invalid ticket: {0}")]
    InvalidTicket(String),

    #[error("Schema not found: {0}")]
    SchemaNotFound(String),

    #[error("Transport error: {0}")]
    Transport(#[from] tonic::Status),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, ArrowFlightError>;
```

**Verification**:
```bash
cargo check
# Should compile cleanly
```

---

### Step 3: Define Flight Ticket Structure (1 hour)

Flight tickets are opaque blobs that identify what data to fetch. We'll encode query information in them.

**File**: `crates/fraiseql-arrow/src/ticket.rs`

```rust
use crate::error::{ArrowFlightError, Result};
use serde::{Deserialize, Serialize};

/// Flight ticket identifying what data to fetch.
///
/// Tickets are serialized as JSON for human readability during development.
/// In production, you might use a more compact format (protobuf, msgpack).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum FlightTicket {
    /// GraphQL query result.
    ///
    /// Example: `{ "type": "GraphQLQuery", "query": "{ users { id name } }" }`
    GraphQLQuery {
        query: String,
        variables: Option<serde_json::Value>,
    },

    /// Observer events stream.
    ///
    /// Example: `{ "type": "ObserverEvents", "entity_type": "Order", "start": "2026-01-01", "limit": 10000 }`
    ObserverEvents {
        entity_type: String,
        start_date: Option<String>,
        end_date: Option<String>,
        limit: Option<usize>,
    },

    /// Bulk data export.
    ///
    /// Example: `{ "type": "BulkExport", "table": "users", "limit": 1000000 }`
    BulkExport {
        table: String,
        filter: Option<String>,
        limit: Option<usize>,
    },
}

impl FlightTicket {
    /// Encode ticket as bytes for Flight protocol.
    pub fn encode(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }

    /// Decode ticket from bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(|e| {
            ArrowFlightError::InvalidTicket(format!("Failed to parse ticket: {}", e))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ticket_roundtrip() {
        let ticket = FlightTicket::GraphQLQuery {
            query: "{ users { id } }".to_string(),
            variables: None,
        };

        let bytes = ticket.encode().unwrap();
        let decoded = FlightTicket::decode(&bytes).unwrap();

        match decoded {
            FlightTicket::GraphQLQuery { query, .. } => {
                assert_eq!(query, "{ users { id } }");
            }
            _ => panic!("Wrong ticket type"),
        }
    }

    #[test]
    fn test_observer_events_ticket() {
        let ticket = FlightTicket::ObserverEvents {
            entity_type: "Order".to_string(),
            start_date: Some("2026-01-01".to_string()),
            end_date: Some("2026-01-31".to_string()),
            limit: Some(10000),
        };

        let bytes = ticket.encode().unwrap();
        let decoded = FlightTicket::decode(&bytes).unwrap();

        match decoded {
            FlightTicket::ObserverEvents { entity_type, limit, .. } => {
                assert_eq!(entity_type, "Order");
                assert_eq!(limit, Some(10000));
            }
            _ => panic!("Wrong ticket type"),
        }
    }
}
```

**Verification**:
```bash
cargo test --lib ticket
# Should show 2 tests passing
```

---

### Step 4: Define Basic Arrow Schemas (1 hour)

**File**: `crates/fraiseql-arrow/src/schema.rs`

```rust
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use std::sync::Arc;

/// Arrow schema for GraphQL query results.
///
/// This is a placeholder - actual schemas will be generated dynamically
/// from GraphQL types in Phase 9.2.
pub fn graphql_result_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("data", DataType::Utf8, false), // JSON for now
    ]))
}

/// Arrow schema for observer events.
///
/// Maps to `EntityEvent` struct from fraiseql-observers.
pub fn observer_event_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("event_id", DataType::Utf8, false),
        Field::new("event_type", DataType::Utf8, false),
        Field::new("entity_type", DataType::Utf8, false),
        Field::new("entity_id", DataType::Utf8, false),
        Field::new(
            "timestamp",
            DataType::Timestamp(TimeUnit::Nanosecond, Some(Arc::from("UTC"))),
            false,
        ),
        Field::new("data", DataType::Utf8, false), // JSON string
        Field::new("user_id", DataType::Utf8, true),
        Field::new("org_id", DataType::Utf8, true),
    ]))
}

/// Arrow schema for bulk exports (table rows).
///
/// This is a placeholder - actual schemas will be generated from database
/// table metadata in Phase 9.4.
pub fn bulk_export_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("data", DataType::Utf8, false), // JSON for now
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_observer_event_schema() {
        let schema = observer_event_schema();
        assert_eq!(schema.fields().len(), 8);
        assert_eq!(schema.field(0).name(), "event_id");
        assert_eq!(schema.field(4).name(), "timestamp");
    }

    #[test]
    fn test_graphql_result_schema() {
        let schema = graphql_result_schema();
        assert_eq!(schema.fields().len(), 2);
    }
}
```

**Verification**:
```bash
cargo test --lib schema
# Should show 2 tests passing
```

---

### Step 5: Implement Flight Server Trait (2-3 hours)

**File**: `crates/fraiseql-arrow/src/flight_server.rs`

```rust
use crate::error::{ArrowFlightError, Result};
use crate::schema::{graphql_result_schema, observer_event_schema};
use crate::ticket::FlightTicket;
use arrow_flight::{
    flight_service_server::{FlightService, FlightServiceServer},
    Action, ActionType, Criteria, Empty, FlightData, FlightDescriptor, FlightInfo,
    HandshakeRequest, HandshakeResponse, PutResult, SchemaResult, Ticket,
};
use futures::Stream;
use std::pin::Pin;
use tonic::{Request, Response, Status, Streaming};
use tracing::{info, warn};

type FlightDataStream = Pin<Box<dyn Stream<Item = Result<FlightData, Status>> + Send>>;

/// FraiseQL Arrow Flight service implementation.
///
/// This is the core gRPC service that handles Flight RPC calls.
/// It will be extended in subsequent phases to actually fetch/stream data.
pub struct FraiseQLFlightService {
    // Future: Will hold references to query executor, observer system, etc.
}

impl FraiseQLFlightService {
    pub fn new() -> Self {
        Self {}
    }

    /// Create a gRPC server from this service.
    pub fn into_server(self) -> FlightServiceServer<Self> {
        FlightServiceServer::new(self)
    }
}

#[tonic::async_trait]
impl FlightService for FraiseQLFlightService {
    type HandshakeStream = FlightDataStream;
    type ListFlightsStream = FlightDataStream;
    type DoGetStream = FlightDataStream;
    type DoPutStream = FlightDataStream;
    type DoActionStream = FlightDataStream;
    type ListActionsStream = FlightDataStream;
    type DoExchangeStream = FlightDataStream;

    /// Handshake for authentication (not implemented yet).
    async fn handshake(
        &self,
        _request: Request<Streaming<HandshakeRequest>>,
    ) -> Result<Response<Self::HandshakeStream>, Status> {
        info!("Handshake called (not implemented)");
        Err(Status::unimplemented("Handshake not implemented yet"))
    }

    /// List available datasets/queries.
    ///
    /// In Phase 9.1, this returns a hardcoded list for testing.
    /// In Phase 9.2+, this will list available GraphQL queries, observer events, etc.
    async fn list_flights(
        &self,
        _request: Request<Criteria>,
    ) -> Result<Response<Self::ListFlightsStream>, Status> {
        info!("ListFlights called");

        // TODO: Return actual available datasets
        // For now, just demonstrate the API works
        let stream = futures::stream::empty();
        Ok(Response::new(Box::pin(stream)))
    }

    /// Get schema for a dataset without fetching data.
    ///
    /// This is used by clients to inspect the schema before fetching data.
    async fn get_schema(
        &self,
        request: Request<FlightDescriptor>,
    ) -> Result<Response<SchemaResult>, Status> {
        let descriptor = request.into_inner();
        info!("GetSchema called: {:?}", descriptor);

        // Decode ticket from descriptor path
        if descriptor.path.is_empty() {
            return Err(Status::invalid_argument("Empty flight descriptor path"));
        }

        let ticket_bytes = descriptor.path[0].as_bytes();
        let ticket = FlightTicket::decode(ticket_bytes)
            .map_err(|e| Status::invalid_argument(format!("Invalid ticket: {}", e)))?;

        // Return appropriate schema based on ticket type
        let schema = match ticket {
            FlightTicket::GraphQLQuery { .. } => graphql_result_schema(),
            FlightTicket::ObserverEvents { .. } => observer_event_schema(),
            FlightTicket::BulkExport { .. } => {
                // Will be implemented in Phase 9.4
                return Err(Status::unimplemented("BulkExport not implemented yet"));
            }
        };

        // Serialize schema to IPC format
        let options = arrow::ipc::writer::IpcWriteOptions::default();
        let schema_bytes = arrow::ipc::writer::IpcDataGenerator::schema_to_bytes(&schema, &options);

        Ok(Response::new(SchemaResult {
            schema: schema_bytes,
        }))
    }

    /// Fetch data stream (main data retrieval method).
    ///
    /// In Phase 9.1, this returns empty streams.
    /// In Phase 9.2+, this will execute queries and stream Arrow RecordBatches.
    async fn do_get(
        &self,
        request: Request<Ticket>,
    ) -> Result<Response<Self::DoGetStream>, Status> {
        let ticket_bytes = request.into_inner().ticket;
        let ticket = FlightTicket::decode(&ticket_bytes)
            .map_err(|e| Status::invalid_argument(format!("Invalid ticket: {}", e)))?;

        info!("DoGet called: {:?}", ticket);

        // TODO: Phase 9.2+ will implement actual data fetching
        // For now, return empty stream to validate server works
        let stream = futures::stream::empty();
        Ok(Response::new(Box::pin(stream)))
    }

    /// Upload data stream (for client-to-server data transfer).
    ///
    /// Not needed for Phase 9.1-9.3 (we're focused on server→client).
    /// May be useful in Phase 9.4+ for bulk imports.
    async fn do_put(
        &self,
        _request: Request<Streaming<FlightData>>,
    ) -> Result<Response<Self::DoPutStream>, Status> {
        warn!("DoPut called but not implemented");
        Err(Status::unimplemented("DoPut not implemented yet"))
    }

    /// Execute an action (RPC method for operations beyond data transfer).
    async fn do_action(
        &self,
        _request: Request<Action>,
    ) -> Result<Response<Self::DoActionStream>, Status> {
        warn!("DoAction called but not implemented");
        Err(Status::unimplemented("DoAction not implemented yet"))
    }

    /// List available actions.
    async fn list_actions(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<Self::ListActionsStream>, Status> {
        info!("ListActions called");
        let stream = futures::stream::empty();
        Ok(Response::new(Box::pin(stream)))
    }

    /// Bidirectional streaming (not needed for FraiseQL use cases).
    async fn do_exchange(
        &self,
        _request: Request<Streaming<FlightData>>,
    ) -> Result<Response<Self::DoExchangeStream>, Status> {
        warn!("DoExchange called but not implemented");
        Err(Status::unimplemented("DoExchange not implemented yet"))
    }
}

impl Default for FraiseQLFlightService {
    fn default() -> Self {
        Self::new()
    }
}
```

**Verification**:
```bash
cargo check
# Should compile cleanly
```

---

### Step 6: Define Library Public API (30 min)

**File**: `crates/fraiseql-arrow/src/lib.rs`

```rust
//! FraiseQL Arrow Flight integration.
//!
//! This crate provides Apache Arrow Flight support for FraiseQL, enabling:
//! - High-performance columnar data transfer (50x faster than JSON)
//! - Zero-copy deserialization in clients (Python, R, Java)
//! - Direct integration with data warehouses (ClickHouse, Snowflake)
//!
//! # Architecture
//!
//! ```text
//! Client                    fraiseql-server              fraiseql-core
//!   │                             │                            │
//!   ├─── DoGet(ticket) ──────────>│                            │
//!   │                             ├─── Execute query ─────────>│
//!   │                             │<─── SQL rows ──────────────┤
//!   │                             ├─── Convert to Arrow ───────│
//!   │<─── Arrow RecordBatch ──────┤                            │
//!   │<─── Arrow RecordBatch ──────┤                            │
//!   │                             │                            │
//! ```
//!
//! # Example
//!
//! ```rust,no_run
//! use fraiseql_arrow::flight_server::FraiseQLFlightService;
//! use tonic::transport::Server;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let service = FraiseQLFlightService::new();
//!     let addr = "0.0.0.0:50051".parse()?;
//!
//!     Server::builder()
//!         .add_service(service.into_server())
//!         .serve(addr)
//!         .await?;
//!
//!     Ok(())
//! }
//! ```

pub mod error;
pub mod flight_server;
pub mod schema;
pub mod ticket;

pub use error::{ArrowFlightError, Result};
pub use flight_server::FraiseQLFlightService;
pub use ticket::FlightTicket;
```

**Verification**:
```bash
cargo doc --no-deps --open
# Should open documentation showing public API
```

---

### Step 7: Integration Test - Server Lifecycle (1-2 hours)

**File**: `crates/fraiseql-arrow/tests/integration_test.rs`

```rust
use arrow_flight::{
    flight_service_client::FlightServiceClient, Criteria, FlightDescriptor, Ticket,
};
use fraiseql_arrow::{flight_server::FraiseQLFlightService, FlightTicket};
use tonic::transport::Server;

/// Start a test Flight server on a random port.
async fn start_test_server() -> Result<String, Box<dyn std::error::Error>> {
    let service = FraiseQLFlightService::new();

    // Use port 0 to get a random available port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    tokio::spawn(async move {
        Server::builder()
            .add_service(service.into_server())
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    Ok(format!("http://127.0.0.1:{}", addr.port()))
}

#[tokio::test]
async fn test_server_starts_and_accepts_connections() {
    let addr = start_test_server().await.unwrap();

    let mut client = FlightServiceClient::connect(addr.clone())
        .await
        .expect("Failed to connect to Flight server");

    // Test ListFlights (should succeed even if empty)
    let request = tonic::Request::new(Criteria { expression: vec![] });
    let response = client.list_flights(request).await;
    assert!(response.is_ok(), "ListFlights should succeed");
}

#[tokio::test]
async fn test_get_schema_for_observer_events() {
    let addr = start_test_server().await.unwrap();

    let mut client = FlightServiceClient::connect(addr)
        .await
        .expect("Failed to connect to Flight server");

    // Create ticket for observer events
    let ticket = FlightTicket::ObserverEvents {
        entity_type: "Order".to_string(),
        start_date: None,
        end_date: None,
        limit: None,
    };

    let ticket_bytes = ticket.encode().unwrap();

    // Request schema
    let descriptor = FlightDescriptor::new_path(vec![String::from_utf8(ticket_bytes).unwrap()]);
    let request = tonic::Request::new(descriptor);

    let response = client.get_schema(request).await.expect("GetSchema failed");
    let schema_result = response.into_inner();

    // Verify we got schema bytes back
    assert!(!schema_result.schema.is_empty(), "Schema should not be empty");

    // Decode and verify schema structure
    let schema = arrow::ipc::root_as_message(&schema_result.schema)
        .expect("Failed to decode schema");
    // Just verify we can decode it - detailed schema checks in Phase 9.2
    assert!(schema.header_type() == arrow::ipc::MessageHeader::Schema);
}

#[tokio::test]
async fn test_do_get_returns_empty_stream() {
    let addr = start_test_server().await.unwrap();

    let mut client = FlightServiceClient::connect(addr)
        .await
        .expect("Failed to connect to Flight server");

    // Create ticket for GraphQL query
    let ticket = FlightTicket::GraphQLQuery {
        query: "{ users { id } }".to_string(),
        variables: None,
    };

    let ticket_bytes = ticket.encode().unwrap();

    // Request data
    let request = tonic::Request::new(Ticket {
        ticket: ticket_bytes,
    });

    let response = client.do_get(request).await.expect("DoGet failed");
    let mut stream = response.into_inner();

    // In Phase 9.1, stream should be empty (no data implementation yet)
    // In Phase 9.2+, this will return actual RecordBatches
    let first_item = stream.message().await.expect("Stream error");
    assert!(first_item.is_none(), "Stream should be empty in Phase 9.1");
}

#[tokio::test]
async fn test_invalid_ticket_returns_error() {
    let addr = start_test_server().await.unwrap();

    let mut client = FlightServiceClient::connect(addr)
        .await
        .expect("Failed to connect to Flight server");

    // Send invalid ticket bytes
    let request = tonic::Request::new(Ticket {
        ticket: b"invalid json".to_vec(),
    });

    let response = client.do_get(request).await;
    assert!(response.is_err(), "Invalid ticket should return error");

    let err = response.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}
```

**Verification**:
```bash
cargo test --test integration_test
# Should show 4 tests passing
```

---

### Step 8: Update Workspace and Server Integration (1 hour)

**File**: `crates/fraiseql-server/Cargo.toml`

Add Arrow Flight as optional feature:

```toml
[features]
default = ["http"]
http = []
arrow = ["fraiseql-arrow"]  # NEW - enables Arrow Flight support

[dependencies]
fraiseql-core = { path = "../fraiseql-core" }
fraiseql-wire = { path = "../fraiseql-wire" }
fraiseql-arrow = { path = "../fraiseql-arrow", optional = true }  # NEW

# ... existing dependencies ...
```

**Verification**:
```bash
cd crates/fraiseql-server
cargo check --features arrow
# Should compile with Arrow Flight support
```

---

## Verification Commands

Run these commands to verify Phase 9.1 is complete:

```bash
# 1. Check all code compiles
cd crates/fraiseql-arrow
cargo check
cargo clippy -- -D warnings

# 2. Run unit tests
cargo test --lib

# 3. Run integration tests
cargo test --test integration_test

# 4. Check documentation
cargo doc --no-deps --open

# 5. Verify workspace integration
cd ../..
cargo check --all-features

# Expected output:
# ✅ All checks pass
# ✅ 6+ tests passing (2 ticket + 2 schema + 4 integration)
# ✅ Zero clippy warnings
# ✅ Documentation builds successfully
```

---

## Acceptance Criteria

Phase 9.1 is complete when:

- ✅ `fraiseql-arrow` crate exists and compiles cleanly
- ✅ Flight server trait implemented with all RPC methods
- ✅ `GetSchema` returns correct Arrow schemas for tickets
- ✅ `DoGet` accepts tickets (returns empty streams for now)
- ✅ `ListFlights` works (returns empty list for now)
- ✅ Integration tests pass (server lifecycle + basic RPC calls)
- ✅ FlightTicket encode/decode works for all ticket types
- ✅ Zero clippy warnings with `clippy::pedantic`
- ✅ Documentation complete with examples
- ✅ `fraiseql-server` can optionally enable Arrow Flight via feature flag

---

## DO NOT

- ❌ **DO NOT** implement actual data fetching yet (Phase 9.2+)
- ❌ **DO NOT** implement authentication/authorization yet (Phase 9.7)
- ❌ **DO NOT** add metrics/observability yet (Phase 8.7 after Phase 9)
- ❌ **DO NOT** integrate with fraiseql-core query executor yet (Phase 9.2)
- ❌ **DO NOT** implement DoPut (data upload) - not needed for Phase 9
- ❌ **DO NOT** worry about performance optimization yet - focus on correctness
- ❌ **DO NOT** add connection pooling or production hardening - Phase 9.1 is foundation only

---

## Next Steps

After Phase 9.1 is complete, proceed to:

**[Phase 9.2: GraphQL Results → Arrow Conversion](./phase-9.2-graphql-to-arrow.md)**

This phase will:
- Implement actual data fetching in `DoGet`
- Convert SQL rows → Arrow RecordBatches
- Map GraphQL types → Arrow schemas dynamically
- Stream large result sets efficiently

---

## Notes

- **gRPC Port**: Use 50051 by default (Flight protocol convention)
- **HTTP + gRPC**: fraiseql-server will run both HTTP (GraphQL) on 8080 and gRPC (Flight) on 50051
- **Feature Flag**: Arrow Flight is optional - users who don't need it won't pay the compilation cost
- **Schemas are Placeholders**: Phase 9.1 uses hardcoded schemas; Phase 9.2+ will generate them dynamically from GraphQL/database schemas

---

**Ready to implement? Start with Step 1: Create `fraiseql-arrow` crate.**
