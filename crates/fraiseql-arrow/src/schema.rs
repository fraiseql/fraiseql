//! Arrow schema definitions for FraiseQL data types.
//!
//! This module provides predefined Arrow schemas for common FraiseQL data structures.
//! In Phase 9.2+, schemas will be generated dynamically from GraphQL types.

use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Schema, TimeUnit};

/// Arrow schema for GraphQL query results.
///
/// This is a placeholder schema used in Phase 9.1.
/// In Phase 9.2, schemas will be generated dynamically from GraphQL type definitions.
///
/// # Schema
///
/// - `id`: UTF-8 string (not nullable)
/// - `data`: UTF-8 string containing JSON (not nullable)
pub fn graphql_result_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("data", DataType::Utf8, false), // JSON for now
    ]))
}

/// Arrow schema for observer events.
///
/// Maps to `EntityEvent` struct from `fraiseql-observers`.
///
/// # Schema
///
/// - `event_id`: Event UUID as UTF-8 string
/// - `event_type`: Event type (e.g., "Order.Created")
/// - `entity_type`: Entity type (e.g., "Order")
/// - `entity_id`: Entity identifier
/// - `timestamp`: Timestamp with microsecond precision in UTC
/// - `data`: Event payload as JSON string
/// - `user_id`: Optional user identifier
/// - `org_id`: Optional organization identifier
pub fn observer_event_schema() -> Arc<Schema> {
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
        Field::new("data", DataType::Utf8, false), // JSON string
        Field::new("user_id", DataType::Utf8, true),
        Field::new("org_id", DataType::Utf8, true),
    ]))
}

/// Arrow schema for bulk exports (table rows).
///
/// This is a placeholder schema used in Phase 9.1.
/// In Phase 9.4, schemas will be generated from database table metadata.
///
/// # Schema
///
/// - `id`: 64-bit integer (not nullable)
/// - `data`: UTF-8 string containing JSON (not nullable)
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
    fn test_observer_event_schema_structure() {
        let schema = observer_event_schema();

        assert_eq!(schema.fields().len(), 8);
        assert_eq!(schema.field(0).name(), "event_id");
        assert_eq!(schema.field(1).name(), "event_type");
        assert_eq!(schema.field(2).name(), "entity_type");
        assert_eq!(schema.field(3).name(), "entity_id");
        assert_eq!(schema.field(4).name(), "timestamp");
        assert_eq!(schema.field(5).name(), "data");
        assert_eq!(schema.field(6).name(), "user_id");
        assert_eq!(schema.field(7).name(), "org_id");

        // Verify nullable fields
        assert!(!schema.field(0).is_nullable()); // event_id
        assert!(schema.field(6).is_nullable()); // user_id
        assert!(schema.field(7).is_nullable()); // org_id
    }

    #[test]
    fn test_observer_event_timestamp_type() {
        let schema = observer_event_schema();
        let timestamp_field = schema.field(4);

        match timestamp_field.data_type() {
            DataType::Timestamp(TimeUnit::Microsecond, Some(tz)) => {
                assert_eq!(tz.as_ref(), "UTC");
            },
            _ => panic!("Expected Timestamp(Microsecond, UTC)"),
        }
    }

    #[test]
    fn test_graphql_result_schema() {
        let schema = graphql_result_schema();
        assert_eq!(schema.fields().len(), 2);
        assert_eq!(schema.field(0).name(), "id");
        assert_eq!(schema.field(1).name(), "data");
    }

    #[test]
    fn test_bulk_export_schema() {
        let schema = bulk_export_schema();
        assert_eq!(schema.fields().len(), 2);
        assert_eq!(schema.field(0).name(), "id");
        assert_eq!(schema.field(0).data_type(), &DataType::Int64);
    }
}
