#![allow(clippy::panic)] // Reason: test code, panics acceptable
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
