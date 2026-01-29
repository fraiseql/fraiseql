//! NATS → Arrow Flight bridge for observer events.
//!
//! This module provides conversion from EntityEvent to Arrow RecordBatches
//! for high-performance event streaming and analytics.

use std::sync::Arc;

use arrow::{
    array::{Array, RecordBatch, StringBuilder, TimestampMicrosecondBuilder},
    error::ArrowError,
};

use crate::event::EntityEvent;

/// Convert a batch of EntityEvents to Arrow RecordBatch.
///
/// Uses the schema from fraiseql_arrow::event_schema::entity_event_arrow_schema().
///
/// # Example
///
/// ```no_run
/// use fraiseql_observers::arrow_bridge::EventToArrowConverter;
/// use fraiseql_observers::event::{EntityEvent, EventKind};
/// use uuid::Uuid;
/// use serde_json::json;
///
/// let events = vec![
///     EntityEvent::new(
///         EventKind::Created,
///         "Order".to_string(),
///         Uuid::new_v4(),
///         json!({"total": 100.50}),
///     ),
/// ];
///
/// let converter = EventToArrowConverter::new(10_000);
/// let batch = converter.convert_events(&events).unwrap();
/// assert_eq!(batch.num_rows(), 1);
/// ```
pub struct EventToArrowConverter {
    _batch_size: usize,
}

impl EventToArrowConverter {
    /// Create a new converter with specified batch size.
    #[must_use]
    pub const fn new(batch_size: usize) -> Self {
        Self { _batch_size: batch_size }
    }

    /// Convert events to Arrow RecordBatch.
    ///
    /// # Errors
    ///
    /// Returns error if Arrow conversion fails or JSON serialization fails.
    pub fn convert_events(&self, events: &[EntityEvent]) -> Result<RecordBatch, ArrowError> {
        if events.is_empty() {
            // Return empty batch with correct schema
            let schema = fraiseql_arrow::event_schema::entity_event_arrow_schema();
            return Ok(RecordBatch::new_empty(schema));
        }

        // Get schema
        let schema = fraiseql_arrow::event_schema::entity_event_arrow_schema();

        // Create builders for each column
        let mut event_id_builder = StringBuilder::with_capacity(events.len(), events.len() * 36);
        let mut event_type_builder = StringBuilder::with_capacity(events.len(), events.len() * 10);
        let mut entity_type_builder = StringBuilder::with_capacity(events.len(), events.len() * 20);
        let mut entity_id_builder = StringBuilder::with_capacity(events.len(), events.len() * 36);
        let mut timestamp_builder = TimestampMicrosecondBuilder::with_capacity(events.len())
            .with_timezone(Arc::from("UTC"));
        let mut data_builder = StringBuilder::with_capacity(events.len(), events.len() * 500);
        let mut user_id_builder = StringBuilder::with_capacity(events.len(), events.len() * 36);
        let mut org_id_builder = StringBuilder::with_capacity(events.len(), events.len() * 36);

        // Populate builders
        for event in events {
            // event_id (UUID as string)
            event_id_builder.append_value(event.id.to_string());

            // event_type (EventKind as string)
            event_type_builder.append_value(event.event_type.as_str());

            // entity_type (string)
            entity_type_builder.append_value(&event.entity_type);

            // entity_id (UUID as string)
            entity_id_builder.append_value(event.entity_id.to_string());

            // timestamp (convert to microseconds since epoch)
            let micros = event.timestamp.timestamp_micros();
            timestamp_builder.append_value(micros);

            // data (serialize JSON to string)
            let data_str = serde_json::to_string(&event.data)
                .map_err(|e| ArrowError::ExternalError(Box::new(e)))?;
            data_builder.append_value(data_str);

            // user_id (nullable)
            match &event.user_id {
                Some(id) => user_id_builder.append_value(id),
                None => user_id_builder.append_null(),
            }

            // org_id (not in EntityEvent, always null for now)
            // TODO: Add org_id to EntityEvent if needed
            org_id_builder.append_null();
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

#[cfg(test)]
mod tests {
    use serde_json::json;
    use uuid::Uuid;

    use super::*;
    use crate::event::EventKind;

    #[test]
    fn test_convert_single_event() {
        let event = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            json!({"total": 100.50}),
        )
        .with_user_id("user-1".to_string());

        let converter = EventToArrowConverter::new(10_000);
        let batch = converter.convert_events(&[event]).unwrap();

        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 8);
    }

    #[test]
    fn test_convert_multiple_events() {
        let events = vec![
            EntityEvent::new(
                EventKind::Created,
                "Order".to_string(),
                Uuid::new_v4(),
                json!({"total": 100.50}),
            )
            .with_user_id("user-1".to_string()),
            EntityEvent::new(
                EventKind::Updated,
                "Order".to_string(),
                Uuid::new_v4(),
                json!({"total": 200.00}),
            ),
            EntityEvent::new(
                EventKind::Deleted,
                "Product".to_string(),
                Uuid::new_v4(),
                json!({"id": 42}),
            ),
        ];

        let converter = EventToArrowConverter::new(10_000);
        let batch = converter.convert_events(&events).unwrap();

        assert_eq!(batch.num_rows(), 3);
        assert_eq!(batch.num_columns(), 8);
    }

    #[test]
    fn test_convert_empty_batch() {
        let converter = EventToArrowConverter::new(10_000);
        let batch = converter.convert_events(&[]).unwrap();

        assert_eq!(batch.num_rows(), 0);
        assert_eq!(batch.num_columns(), 8);
    }

    #[test]
    fn test_null_user_id() {
        let event =
            EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}));

        let converter = EventToArrowConverter::new(10_000);
        let batch = converter.convert_events(&[event]).unwrap();

        assert_eq!(batch.num_rows(), 1);
        // user_id column should have a null value
    }

    #[test]
    fn test_event_types() {
        let events = vec![
            EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({})),
            EntityEvent::new(EventKind::Updated, "Order".to_string(), Uuid::new_v4(), json!({})),
            EntityEvent::new(EventKind::Deleted, "Order".to_string(), Uuid::new_v4(), json!({})),
            EntityEvent::new(EventKind::Custom, "Order".to_string(), Uuid::new_v4(), json!({})),
        ];

        let converter = EventToArrowConverter::new(10_000);
        let batch = converter.convert_events(&events).unwrap();

        assert_eq!(batch.num_rows(), 4);
    }

    #[test]
    fn test_complex_json_data() {
        let event = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            json!({
                "total": 100.50,
                "items": [
                    {"name": "Widget", "price": 50.25},
                    {"name": "Gadget", "price": 50.25}
                ],
                "shipping": {
                    "address": "123 Main St",
                    "city": "Anytown",
                    "country": "US"
                }
            }),
        );

        let converter = EventToArrowConverter::new(10_000);
        let batch = converter.convert_events(&[event]).unwrap();

        assert_eq!(batch.num_rows(), 1);
        // Complex JSON should be serialized to string
    }
}
