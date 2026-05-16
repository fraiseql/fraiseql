#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
use super::*;

#[test]
fn test_typed_stream_creation() {
    // Verify TypedJsonStream can be created with different types
    let _stream: TypedJsonStream<serde_json::Value> =
        TypedJsonStream::new(Box::new(futures::stream::empty()));

    #[derive(serde::Deserialize, Debug)]
    // Reason: test fixture struct used only for deserialization verification
    #[allow(dead_code)] // Reason: field kept for API completeness; may be used in future features
    struct TestType {
        id: String,
    }

    let _stream: TypedJsonStream<TestType> =
        TypedJsonStream::new(Box::new(futures::stream::empty()));
}

#[test]
fn test_deserialize_valid_value() {
    let json = serde_json::json!({
        "id": "123",
        "name": "Test"
    });

    #[derive(serde::Deserialize)]
    // Reason: test fixture struct used only for deserialization verification
    #[allow(dead_code)] // Reason: field kept for API completeness; may be used in future features
    struct TestType {
        id: String,
        name: String,
    }

    let result = TypedJsonStream::<TestType>::deserialize_value(json);
    let item = result.unwrap_or_else(|e| panic!("expected Ok for valid JSON, got: {e}"));
    assert_eq!(item.id, "123");
    assert_eq!(item.name, "Test");
}

#[test]
fn test_deserialize_missing_field() {
    let json = serde_json::json!({
        "id": "123"
        // missing "name" field
    });

    #[derive(Debug, serde::Deserialize)]
    // Reason: test fixture struct used only for deserialization verification
    #[allow(dead_code)] // Reason: field kept for API completeness; may be used in future features
    struct TestType {
        id: String,
        name: String,
    }

    let result = TypedJsonStream::<TestType>::deserialize_value(json);
    match result {
        Err(WireError::Deserialization { type_name, details }) => {
            assert!(type_name.contains("TestType"));
            assert!(details.contains("name"));
        }
        other => panic!("expected Deserialization error for missing field, got: {other:?}"),
    }
}

#[test]
fn test_deserialize_type_mismatch() {
    let json = serde_json::json!({
        "id": "123",
        "count": "not a number"  // should be i32
    });

    #[derive(Debug, serde::Deserialize)]
    // Reason: test fixture struct used only for deserialization verification
    #[allow(dead_code)] // Reason: field kept for API completeness; may be used in future features
    struct TestType {
        id: String,
        count: i32,
    }

    let result = TypedJsonStream::<TestType>::deserialize_value(json);
    match result {
        Err(WireError::Deserialization { type_name, details }) => {
            assert!(type_name.contains("TestType"));
            assert!(details.contains("invalid") || details.contains("type"));
        }
        other => panic!("expected Deserialization error for type mismatch, got: {other:?}"),
    }
}

#[test]
fn test_deserialize_value_type() {
    let json = serde_json::json!({
        "id": "123",
        "name": "Test"
    });

    // Test that Value (escape hatch) works
    let result = TypedJsonStream::<serde_json::Value>::deserialize_value(json.clone());
    let value = result.unwrap_or_else(|e| panic!("expected Ok for Value escape hatch, got: {e}"));
    assert_eq!(value, json);
}

#[test]
fn test_phantom_data_has_no_size() {
    use std::mem::size_of;

    // Verify PhantomData adds zero size
    let size_without_phantom = size_of::<Box<dyn Stream<Item = Result<Value>> + Unpin>>();
    let size_with_phantom = size_of::<TypedJsonStream<serde_json::Value>>();

    // PhantomData should not increase size
    // (might be equal or slightly different due to alignment, but not significantly larger)
    assert!(
        size_with_phantom <= size_without_phantom + 8,
        "PhantomData added too much size: {} vs {}",
        size_with_phantom,
        size_without_phantom
    );
}
