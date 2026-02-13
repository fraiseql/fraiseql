//! Arrow schema for FraiseQL observer events.
//!
//! This module defines the Arrow schema for EntityEvent streaming via Arrow Flight.
//! Events are converted from NATS JetStream to Arrow RecordBatches for high-performance
//! analytics and real-time processing.

use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Schema, TimeUnit};

/// Arrow schema for FraiseQL observer events.
///
/// Maps to `EntityEvent` from fraiseql-observers:
/// ```ignore
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
/// - **event_id**: UUID of the event (Utf8)
/// - **event_type**: Event type (e.g., "Order.Created") (Utf8)
/// - **entity_type**: Entity type (e.g., "Order") (Utf8)
/// - **entity_id**: Entity identifier (Utf8)
/// - **timestamp**: Event timestamp in UTC (Timestamp microseconds)
/// - **data**: Event payload as JSON string (Utf8)
/// - **user_id**: Optional user identifier (Utf8, nullable)
/// - **tenant_id**: Optional tenant identifier for multi-tenant isolation (Utf8, nullable)
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
mod tests {
    use super::*;

    #[test]
    fn test_event_schema_structure() {
        let schema = entity_event_arrow_schema();
        assert_eq!(schema.fields().len(), 8);

        // Check field names
        assert_eq!(schema.field(0).name(), "event_id");
        assert_eq!(schema.field(1).name(), "event_type");
        assert_eq!(schema.field(2).name(), "entity_type");
        assert_eq!(schema.field(3).name(), "entity_id");
        assert_eq!(schema.field(4).name(), "timestamp");
        assert_eq!(schema.field(5).name(), "data");
        assert_eq!(schema.field(6).name(), "user_id");
        assert_eq!(schema.field(7).name(), "tenant_id");
    }

    #[test]
    fn test_nullable_fields() {
        let schema = entity_event_arrow_schema();

        // Required fields
        assert!(!schema.field(0).is_nullable()); // event_id
        assert!(!schema.field(1).is_nullable()); // event_type
        assert!(!schema.field(2).is_nullable()); // entity_type
        assert!(!schema.field(3).is_nullable()); // entity_id
        assert!(!schema.field(4).is_nullable()); // timestamp
        assert!(!schema.field(5).is_nullable()); // data

        // Optional fields
        assert!(schema.field(6).is_nullable()); // user_id
        assert!(schema.field(7).is_nullable()); // tenant_id
    }

    #[test]
    fn test_field_types() {
        let schema = entity_event_arrow_schema();

        // String fields
        assert_eq!(*schema.field(0).data_type(), DataType::Utf8);
        assert_eq!(*schema.field(1).data_type(), DataType::Utf8);
        assert_eq!(*schema.field(2).data_type(), DataType::Utf8);
        assert_eq!(*schema.field(3).data_type(), DataType::Utf8);
        assert_eq!(*schema.field(5).data_type(), DataType::Utf8); // JSON as string
        assert_eq!(*schema.field(6).data_type(), DataType::Utf8);
        assert_eq!(*schema.field(7).data_type(), DataType::Utf8);

        // Timestamp field
        assert_eq!(
            *schema.field(4).data_type(),
            DataType::Timestamp(TimeUnit::Microsecond, Some(Arc::from("UTC")))
        );
    }

    #[test]
    fn test_timestamp_has_utc_timezone() {
        let schema = entity_event_arrow_schema();
        let timestamp_field = schema.field(4);

        if let DataType::Timestamp(unit, tz) = timestamp_field.data_type() {
            assert_eq!(*unit, TimeUnit::Microsecond);
            assert_eq!(tz.as_ref().map(|s| s.as_ref()), Some("UTC"));
        } else {
            panic!("Expected Timestamp type");
        }
    }

    #[test]
    fn test_schema_is_reusable() {
        let schema1 = entity_event_arrow_schema();
        let schema2 = entity_event_arrow_schema();

        // Should create equivalent schemas
        assert_eq!(schema1.fields().len(), schema2.fields().len());
        for (field1, field2) in schema1.fields().iter().zip(schema2.fields().iter()) {
            assert_eq!(field1.name(), field2.name());
            assert_eq!(field1.data_type(), field2.data_type());
            assert_eq!(field1.is_nullable(), field2.is_nullable());
        }
    }
}
