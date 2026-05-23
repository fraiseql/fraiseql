#![allow(clippy::panic)] // Reason: test code, panics acceptable
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
