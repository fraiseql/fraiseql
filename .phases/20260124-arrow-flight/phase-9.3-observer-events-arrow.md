# Phase 9.3: Observer Events → Arrow Streaming

**Duration**: 5-7 days
**Priority**: ⭐⭐⭐⭐⭐
**Dependencies**: Phases 9.1, 9.2 complete
**Status**: Ready to implement (after 9.2)

---

## Objective

Stream observer events from FraiseQL's observer system via Arrow Flight, enabling:
- Real-time event analytics (1M+ events/sec to ClickHouse)
- NATS → Arrow Flight bridge for distributed event sourcing
- Time-range event queries with efficient batching
- Zero-copy event consumption in Python/R for data science

**Use Cases**:
```python
# Real-time analytics on order events
client = flight.connect('grpc://localhost:50051')
ticket = flight.Ticket(b'events:Order:2026-01-01:2026-01-31:streaming')
reader = client.do_get(ticket)

# Process as stream (constant memory)
for batch in reader:
    df = pl.from_arrow(batch)
    # Analyze batch: compute totals, detect anomalies, etc.
    analytics_pipeline.process(df)
```

---

## Context

Currently, observer events flow:
```
PostgreSQL NOTIFY → NATS → ObserverExecutor → Actions (webhooks, emails)
                      ↓
                  Redis (dedup/cache)
```

After Phase 9.3, events also stream via Arrow Flight for analytics:
```
PostgreSQL NOTIFY → NATS → ObserverExecutor → Actions
                      ↓
                  Arrow Flight Server → ClickHouse, Python/R clients
```

**Key Insight**: Reuse existing NATS infrastructure, add Arrow Flight as another consumer.

---

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│               Observer Event Flow                        │
└─────────────────────────────────────────────────────────┘

Database Mutation
    ↓
PostgreSQL NOTIFY
    ↓
NATS JetStream (durable stream)
    ↓
    ├──→ ObserverExecutor (actions: webhooks, emails) [existing]
    │
    └──→ Arrow Flight Bridge (NEW)
         ↓
         Arrow Flight Server
         ↓
         ├──→ ClickHouse (analytics database)
         ├──→ Python clients (real-time data science)
         └──→ R clients (statistical analysis)
```

---

## Files to Create

### 1. Observer Event Bridge

**File**: `crates/fraiseql-observers/src/arrow_bridge.rs`
- Subscribe to NATS stream
- Batch events (10k per RecordBatch)
- Convert EntityEvent → Arrow RecordBatch
- Stream via Flight DoGet

### 2. Event Schema

**File**: `crates/fraiseql-arrow/src/event_schema.rs`
- EntityEvent → Arrow schema mapping
- Efficient JSON data column handling
- Timestamp/UUID conversions

### 3. Integration Tests

**File**: `crates/fraiseql-observers/tests/arrow_streaming_test.rs`
- End-to-end event streaming
- NATS → Arrow conversion
- Batch size validation

---

## Implementation Steps

### Step 1: EntityEvent → Arrow Schema (1 hour)

**File**: `crates/fraiseql-arrow/src/event_schema.rs`

```rust
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use std::sync::Arc;

/// Arrow schema for FraiseQL observer events.
///
/// Maps to `EntityEvent` from fraiseql-observers:
/// ```rust
/// pub struct EntityEvent {
///     pub id: Uuid,
///     pub event_type: String,
///     pub entity_type: String,
///     pub entity_id: String,
///     pub timestamp: DateTime<Utc>,
///     pub data: serde_json::Value,
///     pub user_id: Option<String>,
///     pub org_id: Option<String>,
/// }
/// ```
pub fn entity_event_arrow_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("event_id", DataType::Utf8, false),
        Field::new("event_type", DataType::Utf8, false),
        Field::new("entity_type", DataType::Utf8, false),
        Field::new("entity_id", DataType::Utf8, false),
        Field::new(
            "timestamp",
            DataType::Timestamp(TimeUnit::Microsecond, Some(Arc::from("UTC"))),
            false,
        ),
        Field::new("data", DataType::Utf8, false), // JSON as string
        Field::new("user_id", DataType::Utf8, true),
        Field::new("org_id", DataType::Utf8, true),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_schema_structure() {
        let schema = entity_event_arrow_schema();
        assert_eq!(schema.fields().len(), 8);
        assert_eq!(schema.field(0).name(), "event_id");
        assert_eq!(schema.field(4).name(), "timestamp");
        assert!(schema.field(6).is_nullable()); // user_id
        assert!(!schema.field(0).is_nullable()); // event_id
    }
}
```

---

### Step 2: EntityEvent → Arrow Converter (2-3 hours)

**File**: `crates/fraiseql-observers/src/arrow_bridge.rs`

```rust
use arrow::array::{StringBuilder, TimestampMicrosecondBuilder, RecordBatch};
use arrow::datatypes::TimeUnit;
use fraiseql_arrow::event_schema::entity_event_arrow_schema;
use crate::EntityEvent;
use std::sync::Arc;

/// Convert a batch of EntityEvents to Arrow RecordBatch.
pub struct EventToArrowConverter {
    batch_size: usize,
}

impl EventToArrowConverter {
    pub fn new(batch_size: usize) -> Self {
        Self { batch_size }
    }

    /// Convert events to Arrow RecordBatch.
    pub fn convert_events(&self, events: &[EntityEvent]) -> Result<RecordBatch, ArrowError> {
        let schema = entity_event_arrow_schema();

        // Create builders for each column
        let mut event_id_builder = StringBuilder::with_capacity(events.len(), events.len() * 36);
        let mut event_type_builder = StringBuilder::with_capacity(events.len(), events.len() * 20);
        let mut entity_type_builder = StringBuilder::with_capacity(events.len(), events.len() * 20);
        let mut entity_id_builder = StringBuilder::with_capacity(events.len(), events.len() * 36);
        let mut timestamp_builder = TimestampMicrosecondBuilder::with_capacity(events.len())
            .with_timezone(Arc::from("UTC"));
        let mut data_builder = StringBuilder::with_capacity(events.len(), events.len() * 500);
        let mut user_id_builder = StringBuilder::with_capacity(events.len(), events.len() * 36);
        let mut org_id_builder = StringBuilder::with_capacity(events.len(), events.len() * 36);

        // Populate builders
        for event in events {
            event_id_builder.append_value(event.id.to_string());
            event_type_builder.append_value(&event.event_type);
            entity_type_builder.append_value(&event.entity_type);
            entity_id_builder.append_value(&event.entity_id);

            // Convert timestamp to microseconds
            let micros = event.timestamp.timestamp_micros();
            timestamp_builder.append_value(micros);

            // Serialize JSON data to string
            data_builder.append_value(serde_json::to_string(&event.data)?);

            // Nullable fields
            match &event.user_id {
                Some(id) => user_id_builder.append_value(id),
                None => user_id_builder.append_null(),
            }
            match &event.org_id {
                Some(id) => org_id_builder.append_value(id),
                None => org_id_builder.append_null(),
            }
        }

        // Finish builders and create RecordBatch
        let columns: Vec<Arc<dyn Array>> = vec![
            Arc::new(event_id_builder.finish()),
            Arc::new(event_type_builder.finish()),
            Arc::new(entity_type_builder.finish()),
            Arc::new(entity_id_builder.finish()),
            Arc::new(timestamp_builder.finish()),
            Arc::new(data_builder.finish()),
            Arc::new(user_id_builder.finish()),
            Arc::new(org_id_builder.finish()),
        ];

        RecordBatch::try_new(schema, columns)
    }
}

use arrow::error::ArrowError;
use arrow::array::Array;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn test_convert_events_to_arrow() {
        let events = vec![
            EntityEvent {
                id: Uuid::new_v4(),
                event_type: "Order.Created".to_string(),
                entity_type: "Order".to_string(),
                entity_id: "order-123".to_string(),
                timestamp: Utc::now(),
                data: serde_json::json!({"total": 100.50}),
                user_id: Some("user-1".to_string()),
                org_id: Some("org-1".to_string()),
            },
            EntityEvent {
                id: Uuid::new_v4(),
                event_type: "Order.Shipped".to_string(),
                entity_type: "Order".to_string(),
                entity_id: "order-123".to_string(),
                timestamp: Utc::now(),
                data: serde_json::json!({"carrier": "UPS"}),
                user_id: None,
                org_id: Some("org-1".to_string()),
            },
        ];

        let converter = EventToArrowConverter::new(10_000);
        let batch = converter.convert_events(&events).unwrap();

        assert_eq!(batch.num_rows(), 2);
        assert_eq!(batch.num_columns(), 8);
    }
}
```

---

### Step 3: NATS → Arrow Flight Bridge (3-4 hours)

**File**: `crates/fraiseql-observers/src/arrow_bridge.rs` (continued)

```rust
use async_nats::jetstream;
use futures::StreamExt;
use tokio::sync::mpsc;
use tracing::{info, warn};

/// Bridge between NATS event stream and Arrow Flight.
///
/// Subscribes to NATS JetStream, batches events, and makes them available
/// for Arrow Flight DoGet requests.
pub struct NatsArrowBridge {
    jetstream: jetstream::Context,
    stream_name: String,
    consumer_name: String,
    batch_size: usize,
    converter: EventToArrowConverter,
}

impl NatsArrowBridge {
    pub fn new(
        jetstream: jetstream::Context,
        stream_name: String,
        consumer_name: String,
        batch_size: usize,
    ) -> Self {
        Self {
            jetstream,
            stream_name,
            consumer_name,
            batch_size,
            converter: EventToArrowConverter::new(batch_size),
        }
    }

    /// Start consuming events and converting to Arrow batches.
    ///
    /// Returns a channel receiver that yields Arrow RecordBatches.
    pub async fn start(
        &self,
    ) -> Result<mpsc::Receiver<RecordBatch>, Box<dyn std::error::Error>> {
        let (tx, rx) = mpsc::channel(100); // Buffer up to 100 batches

        let consumer = self
            .jetstream
            .get_consumer_from_stream(&self.consumer_name, &self.stream_name)
            .await?;

        let mut messages = consumer.messages().await?;

        let batch_size = self.batch_size;
        let converter = self.converter.clone();

        // Spawn background task to consume and batch events
        tokio::spawn(async move {
            let mut event_buffer = Vec::with_capacity(batch_size);

            while let Some(msg) = messages.next().await {
                match msg {
                    Ok(msg) => {
                        // Deserialize event
                        match serde_json::from_slice::<EntityEvent>(&msg.payload) {
                            Ok(event) => {
                                event_buffer.push(event);

                                // Convert to Arrow when batch is full
                                if event_buffer.len() >= batch_size {
                                    match converter.convert_events(&event_buffer) {
                                        Ok(batch) => {
                                            if tx.send(batch).await.is_err() {
                                                warn!("Arrow batch receiver dropped");
                                                break;
                                            }
                                            event_buffer.clear();
                                        }
                                        Err(e) => {
                                            warn!("Failed to convert events to Arrow: {}", e);
                                        }
                                    }
                                }

                                // Acknowledge message
                                let _ = msg.ack().await;
                            }
                            Err(e) => {
                                warn!("Failed to deserialize event: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("NATS message error: {}", e);
                    }
                }
            }

            // Flush remaining events
            if !event_buffer.is_empty() {
                if let Ok(batch) = converter.convert_events(&event_buffer) {
                    let _ = tx.send(batch).await;
                }
            }

            info!("NATS Arrow bridge consumer stopped");
        });

        Ok(rx)
    }
}
```

---

### Step 4: Integrate with Flight Server (2 hours)

**File**: `crates/fraiseql-arrow/src/flight_server.rs`

```rust
use tokio::sync::mpsc;

pub struct FraiseQLFlightService {
    // Add NATS bridge channel
    event_stream: Option<mpsc::Receiver<RecordBatch>>,
}

impl FraiseQLFlightService {
    /// Create service with optional event stream.
    pub fn with_event_stream(event_stream: mpsc::Receiver<RecordBatch>) -> Self {
        Self {
            event_stream: Some(event_stream),
        }
    }

    /// Stream observer events via Arrow Flight.
    async fn stream_observer_events(
        &self,
        entity_type: &str,
        start_date: Option<String>,
        end_date: Option<String>,
        limit: Option<usize>,
    ) -> Result<impl Stream<Item = Result<FlightData, Status>>, Status> {
        // TODO: Phase 9.3 - implement time-range filtering
        // For now, stream all events from NATS bridge

        // Convert RecordBatch stream to FlightData stream
        // (serialize using IPC format)

        // Placeholder: will be implemented in Step 4
        let stream = futures::stream::empty();
        Ok(stream)
    }
}

#[tonic::async_trait]
impl FlightService for FraiseQLFlightService {
    async fn do_get(
        &self,
        request: Request<Ticket>,
    ) -> Result<Response<Self::DoGetStream>, Status> {
        let ticket = FlightTicket::decode(&request.into_inner().ticket)?;

        match ticket {
            FlightTicket::GraphQLQuery { .. } => {
                // Phase 9.2 implementation
            }
            FlightTicket::ObserverEvents {
                entity_type,
                start_date,
                end_date,
                limit,
            } => {
                // NEW: Stream observer events
                let stream = self
                    .stream_observer_events(&entity_type, start_date, end_date, limit)
                    .await?;
                Ok(Response::new(Box::pin(stream)))
            }
            FlightTicket::BulkExport { .. } => {
                // Phase 9.4
                Err(Status::unimplemented("Bulk export not implemented yet"))
            }
        }
    }
}
```

---

## Verification Commands

```bash
# 1. Compile observer Arrow bridge
cd crates/fraiseql-observers
cargo check --features arrow

# 2. Run unit tests
cargo test --lib arrow_bridge

# 3. Run integration tests (requires NATS running)
docker run -d --name nats -p 4222:4222 nats:latest
cargo test --test arrow_streaming_test

# 4. Clean up
docker stop nats && docker rm nats

# Expected:
# ✅ 5+ tests passing
# ✅ Events convert to Arrow correctly
# ✅ NATS bridge batches events
```

---

## Acceptance Criteria

- ✅ EntityEvent → Arrow schema mapping complete
- ✅ Event converter handles all EntityEvent fields correctly
- ✅ NATS → Arrow bridge batches events (10k per batch)
- ✅ Flight server streams observer events via DoGet
- ✅ Time-range filtering works (start_date, end_date)
- ✅ Limit parameter respected
- ✅ Null handling for optional fields (user_id, org_id)
- ✅ JSON data serialized efficiently
- ✅ Integration tests passing

---

## Performance Targets

- **Throughput**: 1M+ events/sec to ClickHouse
- **Latency**: <10ms event → Arrow conversion
- **Memory**: Constant (batch size × row size)
- **Batch Size**: 10k events (configurable)

---

## Next Steps

**[Phase 9.4: ClickHouse Integration](./phase-9.4-clickhouse-integration.md)**
