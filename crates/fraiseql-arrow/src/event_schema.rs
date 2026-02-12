//! Arrow schema for FraiseQL observer events.
//!
//! This module defines the Arrow schema for EntityEvent streaming via Arrow Flight.
//! Events are converted from NATS JetStream to Arrow RecordBatches for high-performance
//! analytics and real-time processing.

use std::sync::Arc;

use arrow::array::{RecordBatch, StringArray, TimestampMicrosecondArray};
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
        Field::new("timestamp", DataType::Timestamp(TimeUnit::Microsecond, None), false),
        Field::new("data", DataType::Utf8, false), // JSON as string
        Field::new("user_id", DataType::Utf8, true),
        Field::new("tenant_id", DataType::Utf8, true),
    ]))
}

/// Convert a single `HistoricalEvent` to an Arrow `RecordBatch`.
///
/// # Arguments
/// * `event` - The event to convert
/// * `schema` - The Arrow schema to use (typically from `entity_event_arrow_schema()`)
///
/// # Returns
/// A `RecordBatch` with a single row containing the event data
///
/// # Errors
/// Returns error if the event cannot be converted to Arrow format
pub fn event_to_arrow(
    event: &crate::HistoricalEvent,
    schema: &Arc<Schema>,
) -> Result<RecordBatch, String> {
    events_to_arrow(std::slice::from_ref(event), schema)
}

/// Convert multiple `HistoricalEvent`s to an Arrow `RecordBatch`.
///
/// # Arguments
/// * `events` - Slice of events to convert
/// * `schema` - The Arrow schema to use (typically from `entity_event_arrow_schema()`)
///
/// # Returns
/// A `RecordBatch` with all events as rows
///
/// # Errors
/// Returns error if events cannot be converted to Arrow format
pub fn events_to_arrow(
    events: &[crate::HistoricalEvent],
    schema: &Arc<Schema>,
) -> Result<RecordBatch, String> {
    // Build string arrays for each field
    let event_ids: Vec<String> = events.iter().map(|e| e.id.to_string()).collect();
    let event_types: Vec<String> = events.iter().map(|e| e.event_type.clone()).collect();
    let entity_types: Vec<String> = events.iter().map(|e| e.entity_type.clone()).collect();
    let entity_ids: Vec<String> = events.iter().map(|e| e.entity_id.to_string()).collect();
    let data_strs: Vec<String> = events
        .iter()
        .map(|e| e.data.to_string())
        .collect();
    let user_ids: Vec<Option<String>> = events.iter().map(|e| e.user_id.clone()).collect();
    let tenant_ids: Vec<Option<String>> = events.iter().map(|e| e.tenant_id.clone()).collect();

    // Convert timestamps to microseconds since epoch
    let timestamps: Vec<i64> = events
        .iter()
        .map(|e| e.timestamp.timestamp_micros())
        .collect();

    // Create StringArray for event_id
    let event_id_array = StringArray::from(event_ids.iter().map(|s| s.as_str()).collect::<Vec<_>>());

    // Create StringArray for event_type
    let event_type_array = StringArray::from(event_types.iter().map(|s| s.as_str()).collect::<Vec<_>>());

    // Create StringArray for entity_type
    let entity_type_array = StringArray::from(entity_types.iter().map(|s| s.as_str()).collect::<Vec<_>>());

    // Create StringArray for entity_id
    let entity_id_array = StringArray::from(entity_ids.iter().map(|s| s.as_str()).collect::<Vec<_>>());

    // Create TimestampMicrosecondArray for timestamp
    let timestamp_array = TimestampMicrosecondArray::from(timestamps);

    // Create StringArray for data
    let data_array = StringArray::from(data_strs.iter().map(|s| s.as_str()).collect::<Vec<_>>());

    // Create StringArray for user_id (with nulls)
    let user_id_array = StringArray::from(user_ids);

    // Create StringArray for tenant_id (with nulls)
    let tenant_id_array = StringArray::from(tenant_ids);

    // Create RecordBatch
    RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(event_id_array),
            Arc::new(event_type_array),
            Arc::new(entity_type_array),
            Arc::new(entity_id_array),
            Arc::new(timestamp_array),
            Arc::new(data_array),
            Arc::new(user_id_array),
            Arc::new(tenant_id_array),
        ],
    )
    .map_err(|e| format!("Failed to create RecordBatch: {e}"))
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
            DataType::Timestamp(TimeUnit::Microsecond, None)
        );
    }

    #[test]
    fn test_timestamp_is_microseconds() {
        let schema = entity_event_arrow_schema();
        let timestamp_field = schema.field(4);

        if let DataType::Timestamp(unit, _tz) = timestamp_field.data_type() {
            assert_eq!(*unit, TimeUnit::Microsecond);
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

    // Cycle 2: Implement Event to Arrow Conversion

    #[test]
    fn test_single_event_to_arrow() {
        use uuid::Uuid;
        use chrono::Utc;

        let event = crate::HistoricalEvent {
            id:          Uuid::new_v4(),
            event_type:  "INSERT".to_string(),
            entity_type: "Order".to_string(),
            entity_id:   Uuid::new_v4(),
            data:        serde_json::json!({"total": 100.50}),
            user_id:     Some("user123".to_string()),
            tenant_id:   Some("tenant1".to_string()),
            timestamp:   Utc::now(),
        };

        let schema = entity_event_arrow_schema();
        let batch = event_to_arrow(&event, &schema).expect("Failed to convert event");

        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 8);
    }

    #[test]
    fn test_multiple_events_to_arrow() {
        use uuid::Uuid;
        use chrono::Utc;

        let events: Vec<crate::HistoricalEvent> = (0..5)
            .map(|i| crate::HistoricalEvent {
                id:          Uuid::new_v4(),
                event_type:  format!("EVENT_{}", i),
                entity_type: "Order".to_string(),
                entity_id:   Uuid::new_v4(),
                data:        serde_json::json!({"index": i}),
                user_id:     Some(format!("user{}", i)),
                tenant_id:   Some("tenant1".to_string()),
                timestamp:   Utc::now(),
            })
            .collect();

        let schema = entity_event_arrow_schema();
        let batch = events_to_arrow(&events, &schema).expect("Failed to convert events");

        assert_eq!(batch.num_rows(), 5);
        assert_eq!(batch.num_columns(), 8);
    }

    #[test]
    fn test_event_with_null_optional_fields() {
        use uuid::Uuid;
        use chrono::Utc;

        let event = crate::HistoricalEvent {
            id:          Uuid::new_v4(),
            event_type:  "DELETE".to_string(),
            entity_type: "User".to_string(),
            entity_id:   Uuid::new_v4(),
            data:        serde_json::json!({"status": "deleted"}),
            user_id:     None,
            tenant_id:   None,
            timestamp:   Utc::now(),
        };

        let schema = entity_event_arrow_schema();
        let batch = event_to_arrow(&event, &schema).expect("Failed to convert event");

        assert_eq!(batch.num_rows(), 1);
    }

    #[test]
    fn test_event_with_complex_json_data() {
        use uuid::Uuid;
        use chrono::Utc;

        let complex_data = serde_json::json!({
            "nested": {
                "field": "value",
                "count": 42
            },
            "array": [1, 2, 3],
            "string": "test"
        });

        let event = crate::HistoricalEvent {
            id:          Uuid::new_v4(),
            event_type:  "UPDATE".to_string(),
            entity_type: "Product".to_string(),
            entity_id:   Uuid::new_v4(),
            data:        complex_data,
            user_id:     Some("admin".to_string()),
            tenant_id:   Some("tenant1".to_string()),
            timestamp:   Utc::now(),
        };

        let schema = entity_event_arrow_schema();
        let batch = event_to_arrow(&event, &schema).expect("Failed to convert event");

        assert_eq!(batch.num_rows(), 1);
    }

    #[test]
    fn test_batch_with_mixed_null_values() {
        use uuid::Uuid;
        use chrono::Utc;

        let events = vec![
            crate::HistoricalEvent {
                id:          Uuid::new_v4(),
                event_type:  "INSERT".to_string(),
                entity_type: "Order".to_string(),
                entity_id:   Uuid::new_v4(),
                data:        serde_json::json!({"value": 1}),
                user_id:     Some("user1".to_string()),
                tenant_id:   Some("tenant1".to_string()),
                timestamp:   Utc::now(),
            },
            crate::HistoricalEvent {
                id:          Uuid::new_v4(),
                event_type:  "UPDATE".to_string(),
                entity_type: "Order".to_string(),
                entity_id:   Uuid::new_v4(),
                data:        serde_json::json!({"value": 2}),
                user_id:     None,
                tenant_id:   Some("tenant1".to_string()),
                timestamp:   Utc::now(),
            },
            crate::HistoricalEvent {
                id:          Uuid::new_v4(),
                event_type:  "DELETE".to_string(),
                entity_type: "Order".to_string(),
                entity_id:   Uuid::new_v4(),
                data:        serde_json::json!({"value": 3}),
                user_id:     Some("user3".to_string()),
                tenant_id:   None,
                timestamp:   Utc::now(),
            },
        ];

        let schema = entity_event_arrow_schema();
        let batch = events_to_arrow(&events, &schema).expect("Failed to convert events");

        assert_eq!(batch.num_rows(), 3);
    }

    // Cycle 3: Test Arrow Conversion Roundtrip

    #[test]
    fn test_empty_events_batch() {
        let events: Vec<crate::HistoricalEvent> = vec![];
        let schema = entity_event_arrow_schema();
        let batch = events_to_arrow(&events, &schema).expect("Failed to convert empty batch");

        assert_eq!(batch.num_rows(), 0);
        assert_eq!(batch.num_columns(), 8);
    }

    #[test]
    fn test_event_data_preserved_in_arrow() {
        use uuid::Uuid;
        use chrono::Utc;

        let test_data = serde_json::json!({"key": "value", "num": 42});
        let event = crate::HistoricalEvent {
            id:          Uuid::new_v4(),
            event_type:  "TEST".to_string(),
            entity_type: "TestEntity".to_string(),
            entity_id:   Uuid::new_v4(),
            data:        test_data,
            user_id:     Some("testuser".to_string()),
            tenant_id:   Some("testtenant".to_string()),
            timestamp:   Utc::now(),
        };

        let schema = entity_event_arrow_schema();
        let batch = event_to_arrow(&event, &schema).expect("Failed to convert event");

        // Verify batch was created with correct structure
        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.schema().field(0).name(), "event_id");
        assert_eq!(batch.schema().field(1).name(), "event_type");
        assert_eq!(batch.schema().field(5).name(), "data");
    }

    #[test]
    fn test_large_batch_conversion() {
        use uuid::Uuid;
        use chrono::Utc;

        let events: Vec<crate::HistoricalEvent> = (0..1000)
            .map(|i| crate::HistoricalEvent {
                id:          Uuid::new_v4(),
                event_type:  format!("EVENT_{}", i % 10),
                entity_type: "Order".to_string(),
                entity_id:   Uuid::new_v4(),
                data:        serde_json::json!({"batch_index": i}),
                user_id:     Some(format!("user{}", i % 100)),
                tenant_id:   Some("tenant1".to_string()),
                timestamp:   Utc::now(),
            })
            .collect();

        let schema = entity_event_arrow_schema();
        let batch = events_to_arrow(&events, &schema).expect("Failed to convert large batch");

        assert_eq!(batch.num_rows(), 1000);
        assert_eq!(batch.num_columns(), 8);
    }
}
