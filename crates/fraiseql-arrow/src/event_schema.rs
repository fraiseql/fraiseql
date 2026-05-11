//! Arrow schema for FraiseQL observer events.
//!
//! This module defines the Arrow schema for `EntityEvent` streaming via Arrow Flight.
//! Events are converted from NATS `JetStream` to Arrow `RecordBatches` for high-performance
//! analytics and real-time processing.

use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Schema, TimeUnit};

/// Arrow schema for FraiseQL observer events.
///
/// Maps to `EntityEvent` from fraiseql-observers:
/// ```text
/// pub struct EntityEvent {
///     pub id: Uuid,                      // UUID of the event
///     pub event_type: String,            // Event type (e.g., "Order.Created")
///     pub entity_type: String,           // Entity type (e.g., "Order")
///     pub entity_id: String,             // Entity ID
///     pub timestamp: DateTime<Utc>,      // Event timestamp
///     pub data: serde_json::Value,       // Event payload
///     pub user_id: Option<String>,       // User who triggered event
///     pub tenant_id: Option<String>,     // Tenant for multi-tenant isolation
/// }
/// ```
///
/// # Schema Fields
///
/// - **`event_id`**: UUID of the event (Utf8)
/// - **`event_type`**: Event type (e.g., "Order.Created") (Utf8)
/// - **`entity_type`**: Entity type (e.g., "Order") (Utf8)
/// - **`entity_id`**: Entity identifier (Utf8)
/// - **timestamp**: Event timestamp in UTC (Timestamp microseconds)
/// - **data**: Event payload as JSON string (Utf8)
/// - **`user_id`**: Optional user identifier (Utf8, nullable)
/// - **`tenant_id`**: Optional tenant identifier for multi-tenant isolation (Utf8, nullable)
///
/// # Example
///
/// ```
/// use fraiseql_arrow::event_schema::entity_event_arrow_schema;
///
/// let schema = entity_event_arrow_schema();
/// assert_eq!(schema.fields().len(), 8);
/// assert_eq!(schema.field(0).name(), "event_id");
/// ```
#[must_use]
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
        Field::new("tenant_id", DataType::Utf8, true),
    ]))
}

#[cfg(test)]
mod tests;
