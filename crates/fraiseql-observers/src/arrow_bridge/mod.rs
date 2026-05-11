//! NATS → Arrow Flight bridge for observer events.
//!
//! This module provides conversion from `EntityEvent` to Arrow `RecordBatch`es
//! for high-performance event streaming and analytics.

use std::sync::Arc;

use arrow::{
    array::{Array, RecordBatch, StringBuilder, TimestampMicrosecondBuilder},
    error::ArrowError,
};

use crate::event::EntityEvent;

/// Convert a batch of `EntityEvent`s to Arrow `RecordBatch`.
///
/// Uses the schema from `fraiseql_arrow::event_schema::entity_event_arrow_schema()`.
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
        Self {
            _batch_size: batch_size,
        }
    }

    /// Convert events to Arrow `RecordBatch`.
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
        let mut tenant_id_builder = StringBuilder::with_capacity(events.len(), events.len() * 36);

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

            // tenant_id (nullable)
            match &event.tenant_id {
                Some(id) => tenant_id_builder.append_value(id),
                None => tenant_id_builder.append_null(),
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
            Arc::new(tenant_id_builder.finish()),
        ];

        RecordBatch::try_new(schema, columns)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests;
