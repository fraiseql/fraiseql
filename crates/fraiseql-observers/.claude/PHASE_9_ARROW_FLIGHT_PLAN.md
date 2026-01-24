# Phase 9: Apache Arrow Flight Integration - Detailed Plan

**Date**: January 24, 2026
**Status**: Planning Phase
**Dependencies**: Phase 8.7 (Prometheus Metrics)
**Effort**: 3-4 weeks

---

## Executive Summary

Integrate Apache Arrow Flight as a high-performance data delivery layer for:
1. Observer event streaming to analytics platforms
2. GraphQL query results in columnar format (handled in fraiseql-core)
3. Bulk data exports for data warehouses

This plan focuses on **observer-specific Arrow Flight integration**. See main roadmap for full-stack integration.

---

## Architecture: Observer Events via Arrow Flight

```
┌────────────────────────────────────────────────────────┐
│        Observer Event Delivery Comparison               │
└────────────────────────────────────────────────────────┘

Current (NATS + JSON):
PostgreSQL → Observer → NATS → Worker → JSON consumer
  10,000 events/sec, 50MB/sec bandwidth

With Arrow Flight (columnar):
PostgreSQL → Observer → Arrow Flight Server → Analytics
  1,000,000+ events/sec, 5MB/sec bandwidth (10x compression)

Benefits:
✅ 100x throughput improvement
✅ 10x bandwidth reduction (columnar compression)
✅ Zero-copy consumption (Pandas/Polars)
✅ Direct warehouse integration (ClickHouse, Snowflake)
```

---

## Phase 9.1: Foundation (Week 1)

### Objective
Set up Arrow Flight infrastructure in `fraiseql-observers`

### Deliverables

#### 9.1.1: Add Arrow Dependencies
```toml
# Cargo.toml additions
[dependencies]
arrow = { version = "53", features = ["prettyprint"] }
arrow-flight = "53"
arrow-schema = "53"
tonic = { version = "0.12", features = ["tls"] }
prost = "0.13"

[features]
arrow-flight = ["arrow", "arrow-flight", "arrow-schema", "tonic", "prost"]
```

#### 9.1.2: Create `src/arrow_flight/mod.rs`
```rust
//! Apache Arrow Flight server for observer event streaming
//!
//! Provides high-performance columnar event delivery to analytics platforms.
//!
//! # Features
//! - Real-time event streaming
//! - Batch event exports
//! - Zero-copy data transfer
//! - Cross-language client support (Python/R/Java)

#[cfg(feature = "arrow-flight")]
pub mod server;

#[cfg(feature = "arrow-flight")]
pub mod schema;

#[cfg(feature = "arrow-flight")]
pub mod converter;

// Re-export main types
#[cfg(feature = "arrow-flight")]
pub use server::ObserverFlightServer;

#[cfg(feature = "arrow-flight")]
pub use schema::event_schema;

#[cfg(feature = "arrow-flight")]
pub use converter::EntityEventConverter;
```

#### 9.1.3: Design Arrow Schema for EntityEvent
```rust
// src/arrow_flight/schema.rs

use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use std::sync::Arc;

/// Arrow schema for EntityEvent
///
/// Maps FraiseQL EntityEvent to Apache Arrow columnar format:
/// - event_id: Utf8 (UUID string)
/// - event_type: Utf8 (Created/Updated/Deleted)
/// - entity_type: Utf8 (Order, User, etc.)
/// - entity_id: Utf8 (UUID string)
/// - timestamp: Timestamp(Nanosecond, UTC)
/// - data: Utf8 (JSON string) or Struct (parsed)
/// - user_id: Utf8 (optional)
/// - org_id: Utf8 (optional)
pub fn event_schema() -> Arc<Schema> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_schema_structure() {
        let schema = event_schema();
        assert_eq!(schema.fields().len(), 8);
        assert_eq!(schema.field(0).name(), "event_id");
        assert!(!schema.field(0).is_nullable());
    }
}
```

#### 9.1.4: Basic Flight Server Skeleton
```rust
// src/arrow_flight/server.rs

use arrow_flight::{
    flight_service_server::{FlightService, FlightServiceServer},
    Action, ActionType, Criteria, Empty, FlightData, FlightDescriptor,
    FlightInfo, HandshakeRequest, HandshakeResponse, PutResult, SchemaResult,
    Ticket,
};
use tonic::{Request, Response, Status, Streaming};
use futures::Stream;
use std::pin::Pin;

/// Observer Flight Server
///
/// Streams observer events as Apache Arrow RecordBatches.
pub struct ObserverFlightServer {
    // Event source (to be implemented)
}

#[tonic::async_trait]
impl FlightService for ObserverFlightServer {
    type HandshakeStream = Pin<Box<dyn Stream<Item = Result<HandshakeResponse, Status>> + Send>>;
    type ListFlightsStream = Pin<Box<dyn Stream<Item = Result<FlightInfo, Status>> + Send>>;
    type DoGetStream = Pin<Box<dyn Stream<Item = Result<FlightData, Status>> + Send>>;
    type DoPutStream = Pin<Box<dyn Stream<Item = Result<PutResult, Status>> + Send>>;
    type DoActionStream = Pin<Box<dyn Stream<Item = Result<arrow_flight::Result, Status>> + Send>>;
    type ListActionsStream = Pin<Box<dyn Stream<Item = Result<ActionType, Status>> + Send>>;
    type DoExchangeStream = Pin<Box<dyn Stream<Item = Result<FlightData, Status>> + Send>>;

    async fn handshake(
        &self,
        _request: Request<Streaming<HandshakeRequest>>,
    ) -> Result<Response<Self::HandshakeStream>, Status> {
        Err(Status::unimplemented("handshake not yet implemented"))
    }

    async fn list_flights(
        &self,
        _request: Request<Criteria>,
    ) -> Result<Response<Self::ListFlightsStream>, Status> {
        Err(Status::unimplemented("list_flights not yet implemented"))
    }

    async fn get_flight_info(
        &self,
        _request: Request<FlightDescriptor>,
    ) -> Result<Response<FlightInfo>, Status> {
        Err(Status::unimplemented("get_flight_info not yet implemented"))
    }

    async fn get_schema(
        &self,
        _request: Request<FlightDescriptor>,
    ) -> Result<Response<SchemaResult>, Status> {
        Err(Status::unimplemented("get_schema not yet implemented"))
    }

    async fn do_get(
        &self,
        _request: Request<Ticket>,
    ) -> Result<Response<Self::DoGetStream>, Status> {
        Err(Status::unimplemented("do_get not yet implemented"))
    }

    async fn do_put(
        &self,
        _request: Request<Streaming<FlightData>>,
    ) -> Result<Response<Self::DoPutStream>, Status> {
        Err(Status::unimplemented("do_put not yet implemented"))
    }

    async fn do_action(
        &self,
        _request: Request<Action>,
    ) -> Result<Response<Self::DoActionStream>, Status> {
        Err(Status::unimplemented("do_action not yet implemented"))
    }

    async fn list_actions(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<Self::ListActionsStream>, Status> {
        Err(Status::unimplemented("list_actions not yet implemented"))
    }

    async fn do_exchange(
        &self,
        _request: Request<Streaming<FlightData>>,
    ) -> Result<Response<Self::DoExchangeStream>, Status> {
        Err(Status::unimplemented("do_exchange not yet implemented"))
    }
}
```

### Verification
```bash
cargo check --features arrow-flight
cargo test --features arrow-flight test_event_schema
```

---

## Phase 9.2: Event → Arrow Conversion (Week 1-2)

### Objective
Convert `EntityEvent` to Arrow RecordBatches

### Deliverables

#### 9.2.1: EntityEvent → Arrow Converter
```rust
// src/arrow_flight/converter.rs

use crate::event::EntityEvent;
use arrow::array::{
    ArrayRef, StringArray, TimestampNanosecondArray,
};
use arrow::record_batch::RecordBatch;
use std::sync::Arc;

pub struct EntityEventConverter;

impl EntityEventConverter {
    /// Convert a batch of events to Arrow RecordBatch
    pub fn events_to_batch(events: &[EntityEvent]) -> Result<RecordBatch> {
        let mut event_ids = Vec::with_capacity(events.len());
        let mut event_types = Vec::with_capacity(events.len());
        let mut entity_types = Vec::with_capacity(events.len());
        let mut entity_ids = Vec::with_capacity(events.len());
        let mut timestamps = Vec::with_capacity(events.len());
        let mut data_json = Vec::with_capacity(events.len());
        let mut user_ids = Vec::with_capacity(events.len());
        let mut org_ids = Vec::with_capacity(events.len());

        for event in events {
            event_ids.push(event.id.to_string());
            event_types.push(format!("{:?}", event.event_type));
            entity_types.push(event.entity_type.clone());
            entity_ids.push(event.entity_id.to_string());
            timestamps.push(event.timestamp.timestamp_nanos_opt().unwrap_or(0));
            data_json.push(event.data.to_string());
            user_ids.push(event.user_id.clone());
            org_ids.push(event.org_id.clone());
        }

        let columns: Vec<ArrayRef> = vec![
            Arc::new(StringArray::from(event_ids)),
            Arc::new(StringArray::from(event_types)),
            Arc::new(StringArray::from(entity_types)),
            Arc::new(StringArray::from(entity_ids)),
            Arc::new(TimestampNanosecondArray::from(timestamps).with_timezone("UTC")),
            Arc::new(StringArray::from(data_json)),
            Arc::new(StringArray::from(user_ids)),
            Arc::new(StringArray::from(org_ids)),
        ];

        RecordBatch::try_new(super::schema::event_schema(), columns)
            .map_err(|e| ObserverError::InvalidConfig {
                message: format!("Failed to create RecordBatch: {}", e),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventKind;
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn test_events_to_batch() {
        let events = vec![
            EntityEvent::new(
                EventKind::Created,
                "Order".to_string(),
                Uuid::new_v4(),
                json!({"total": 100}),
            ),
        ];

        let batch = EntityEventConverter::events_to_batch(&events).unwrap();
        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 8);
    }
}
```

#### 9.2.2: Streaming Batches
```rust
/// Stream events in batches of configurable size
pub struct EventBatchStream {
    events: Vec<EntityEvent>,
    batch_size: usize,
    position: usize,
}

impl EventBatchStream {
    pub fn new(events: Vec<EntityEvent>, batch_size: usize) -> Self {
        Self {
            events,
            batch_size,
            position: 0,
        }
    }

    pub async fn next_batch(&mut self) -> Result<Option<RecordBatch>> {
        if self.position >= self.events.len() {
            return Ok(None);
        }

        let end = (self.position + self.batch_size).min(self.events.len());
        let batch_events = &self.events[self.position..end];
        self.position = end;

        let batch = EntityEventConverter::events_to_batch(batch_events)?;
        Ok(Some(batch))
    }
}
```

### Verification
```bash
cargo test --features arrow-flight test_events_to_batch
cargo bench --features arrow-flight bench_arrow_conversion
```

**Performance Target**: 100,000 events/sec conversion rate

---

## Phase 9.3: Implement Flight DoGet (Week 2)

### Objective
Stream events via Flight DoGet RPC

### Ticket Format
```
Format: "events:<entity_type>:<start_time>:<end_time>:<batch_size>"

Examples:
- "events:Order:2026-01-01T00:00:00Z:2026-01-31T23:59:59Z:10000"
- "events:*:now-1h:now:5000"  (all types, last hour)
- "events:User:now-1d:now:1000"  (users, last day)
```

### Implementation
```rust
async fn do_get(
    &self,
    request: Request<Ticket>,
) -> Result<Response<Self::DoGetStream>, Status> {
    let ticket = request.into_inner();
    let ticket_str = String::from_utf8_lossy(&ticket.ticket);

    // Parse ticket
    let params = self.parse_ticket(&ticket_str)?;

    // Fetch events from storage/stream
    let events = self.fetch_events(params).await?;

    // Convert to Arrow batches
    let stream = EventBatchStream::new(events, params.batch_size);

    // Stream FlightData
    let flight_stream = self.batch_stream_to_flight_stream(stream);

    Ok(Response::new(Box::pin(flight_stream)))
}
```

### Verification
```bash
# Integration test with real Flight client
cargo test --features arrow-flight test_do_get_integration
```

---

## Phase 9.4: Client Examples (Week 3)

### Python Client
```python
# examples/python/flight_observer_client.py

from pyarrow import flight
import polars as pl
from datetime import datetime, timedelta

# Connect to FraiseQL Flight server
client = flight.connect("grpc://localhost:50051")

# Fetch last hour of Order events
start = (datetime.utcnow() - timedelta(hours=1)).isoformat()
end = datetime.utcnow().isoformat()
ticket_str = f"events:Order:{start}:{end}:10000"
ticket = flight.Ticket(ticket_str)

# Stream events
reader = client.do_get(ticket)
table = reader.read_all()

# Convert to Polars (zero-copy)
df = pl.from_arrow(table)

# Analyze
print(f"Received {len(df)} events")
print(df.head())

# Aggregate
summary = df.group_by("entity_type").agg([
    pl.count().alias("event_count"),
    pl.col("timestamp").min().alias("first_event"),
    pl.col("timestamp").max().alias("last_event"),
])
print(summary)
```

### ClickHouse Integration
```python
# examples/python/clickhouse_ingestion.py

from pyarrow import flight
import clickhouse_connect

# Fetch events from Flight
client = flight.connect("grpc://localhost:50051")
ticket = flight.Ticket("events:*:now-1h:now:10000")
reader = client.do_get(ticket)
table = reader.read_all()

# Insert into ClickHouse (direct Arrow support)
ch_client = clickhouse_connect.get_client(host='localhost', port=8123)
ch_client.insert_arrow('observer_events', table)
```

---

## Phase 9.5: Integration with Existing Observers (Week 3-4)

### Dual Transport Mode

Allow observers to send events to **both** NATS and Arrow Flight:

```rust
pub enum EventDeliveryMode {
    NatsOnly,
    FlightOnly,
    Dual,  // Send to both
}

pub struct ObserverRuntimeConfig {
    // ... existing fields

    #[cfg(feature = "arrow-flight")]
    pub arrow_flight: Option<ArrowFlightConfig>,

    pub delivery_mode: EventDeliveryMode,
}

#[cfg(feature = "arrow-flight")]
pub struct ArrowFlightConfig {
    pub enabled: bool,
    pub bind_address: String,  // "0.0.0.0:50051"
    pub batch_size: usize,     // 10000
    pub tls_cert: Option<String>,
    pub tls_key: Option<String>,
}
```

### Updated ObserverExecutor

```rust
impl ObserverExecutor {
    pub async fn process_event(&self, event: &EntityEvent) -> Result<ExecutionSummary> {
        // ... existing processing logic

        // Send to Arrow Flight buffer (if enabled)
        #[cfg(feature = "arrow-flight")]
        if let Some(ref flight_sink) = self.flight_sink {
            flight_sink.buffer_event(event.clone()).await?;
        }

        // ... continue with actions
    }
}
```

---

## Phase 9.6: Testing & Benchmarks (Week 4)

### Integration Tests
```rust
#[cfg(all(test, feature = "arrow-flight"))]
mod integration_tests {
    #[tokio::test]
    async fn test_flight_server_lifecycle() {
        // Start Flight server
        // Send test events
        // Fetch via Flight client
        // Verify data correctness
    }

    #[tokio::test]
    async fn test_batch_streaming() {
        // Stream 1M events
        // Verify batching works correctly
        // Check memory usage
    }
}
```

### Benchmarks
```rust
// benches/arrow_flight_benchmarks.rs

fn bench_event_conversion(c: &mut Criterion) {
    c.bench_function("convert_10k_events_to_arrow", |b| {
        let events = generate_test_events(10_000);
        b.iter(|| {
            EntityEventConverter::events_to_batch(&events)
        });
    });
}

fn bench_flight_streaming(c: &mut Criterion) {
    c.bench_function("stream_1m_events_via_flight", |b| {
        // Benchmark full Flight streaming pipeline
    });
}
```

**Performance Targets**:
- Conversion: 100,000 events/sec
- Streaming: 1,000,000 events/sec
- Memory: <100MB for 1M events buffered
- Latency: <10ms first batch

---

## Documentation Updates

### Update DEPLOYMENT.md
Add Arrow Flight deployment topology:

```yaml
# docker-compose.arrow-flight.yml

services:
  postgres:
    # ... existing

  observer-flight:
    image: fraiseql-observer:latest
    ports:
      - "50051:50051"  # Flight gRPC
      - "8080:8080"    # HTTP metrics
    environment:
      FRAISEQL_ARROW_FLIGHT_ENABLED: "true"
      FRAISEQL_ARROW_FLIGHT_BIND: "0.0.0.0:50051"
      FRAISEQL_ARROW_FLIGHT_BATCH_SIZE: "10000"
    volumes:
      - ./examples/arrow-flight.toml:/etc/fraiseql/config.toml:ro
```

### Client Integration Guide
```markdown
# Consuming Observer Events via Arrow Flight

## Python (Pandas)
[example code]

## Python (Polars)
[example code]

## R (arrow package)
[example code]

## Java (Arrow Flight Java)
[example code]
```

---

## Success Criteria

### Functional
- ✅ Flight server starts and accepts connections
- ✅ Events converted to Arrow RecordBatches
- ✅ Streaming batches work (10K+ events)
- ✅ Python client can consume events
- ✅ Data correctness validated (all fields match)

### Performance
- ✅ 100,000+ events/sec conversion rate
- ✅ 1,000,000+ events/sec streaming throughput
- ✅ <10ms first batch latency
- ✅ <100MB memory for 1M events buffered
- ✅ 10x bandwidth reduction vs JSON

### Documentation
- ✅ Architecture diagram
- ✅ Client integration examples (Python/R/Java)
- ✅ Deployment guide with Docker Compose
- ✅ Performance tuning guide

---

## Risks & Mitigation

| Risk | Mitigation |
|------|------------|
| Arrow dependency size (~50MB) | Feature-gated, optional |
| gRPC complexity | Use tonic (mature Rust gRPC) |
| Streaming backpressure | Buffering + flow control |
| TLS certificate management | Document clearly, provide examples |
| Client compatibility | Test with multiple client versions |

---

## Next Steps

1. ✅ Review and approve this plan
2. ✅ Complete Phase 8.7 (Prometheus Metrics)
3. ✅ Start Phase 9.1 implementation
4. ✅ Prototype basic Flight server (1-2 days)
5. ✅ Evaluate performance before full commitment

---

**Status**: Awaiting approval
**Owner**: TBD
**Start Date**: After Phase 8.7 completion
**Target Completion**: Q2 2026
