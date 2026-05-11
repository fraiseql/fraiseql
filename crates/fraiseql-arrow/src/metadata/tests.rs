#![allow(clippy::unwrap_used)] // Reason: test code extensively uses unwrap for test fixture setup

use arrow::datatypes::{DataType, Field};

use super::*;

#[test]
fn test_register_and_get_schema() {
    let registry = SchemaRegistry::new();

    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("name", DataType::Utf8, false),
    ]));

    registry.register("va_test", schema);

    let retrieved = registry.get("va_test").unwrap();
    assert_eq!(retrieved.fields().len(), 2);
    assert_eq!(retrieved.field(0).name(), "id");
}

#[test]
fn test_schema_not_found() {
    let registry = SchemaRegistry::new();

    let result = registry.get("nonexistent");
    assert!(result.is_err(), "expected Err, got: {result:?}");
    assert!(result.unwrap_err().to_string().contains("No schema registered"));
}

#[test]
fn test_contains() {
    let registry = SchemaRegistry::new();

    let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

    assert!(!registry.contains("va_test"));
    registry.register("va_test", schema);
    assert!(registry.contains("va_test"));
}

#[test]
fn test_remove() {
    let registry = SchemaRegistry::new();

    let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

    registry.register("va_test", schema);
    assert!(registry.contains("va_test"));

    let removed = registry.remove("va_test");
    assert!(removed.is_some());
    assert!(!registry.contains("va_test"));
}

#[test]
fn test_len_and_is_empty() {
    let registry = SchemaRegistry::new();

    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);

    let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

    registry.register("va_test1", schema.clone());
    assert_eq!(registry.len(), 1);
    assert!(!registry.is_empty());

    registry.register("va_test2", schema);
    assert_eq!(registry.len(), 2);
}

#[test]
fn test_clear() {
    let registry = SchemaRegistry::new();

    let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

    registry.register("va_test1", schema.clone());
    registry.register("va_test2", schema);
    assert_eq!(registry.len(), 2);

    registry.clear();
    assert!(registry.is_empty());
}

#[test]
fn test_register_defaults() {
    let registry = SchemaRegistry::new();

    registry.register_defaults();

    assert!(registry.contains("va_orders"));
    assert!(registry.contains("va_users"));

    let orders_schema = registry.get("va_orders").unwrap();
    assert_eq!(orders_schema.fields().len(), 4);
    assert_eq!(orders_schema.field(0).name(), "id");
    assert_eq!(orders_schema.field(1).name(), "total");

    let users_schema = registry.get("va_users").unwrap();
    assert_eq!(users_schema.fields().len(), 4);
    assert_eq!(users_schema.field(0).name(), "id");
    assert_eq!(users_schema.field(1).name(), "email");
}

#[test]
fn test_register_ta_tables() {
    let registry = SchemaRegistry::new();

    registry.register_ta_tables();

    // Verify ta_orders is registered
    assert!(registry.contains("ta_orders"));
    let ta_orders_schema = registry.get("ta_orders").unwrap();
    assert_eq!(ta_orders_schema.fields().len(), 4);
    assert_eq!(ta_orders_schema.field(0).name(), "id");
    assert_eq!(ta_orders_schema.field(1).name(), "total");
    assert_eq!(ta_orders_schema.field(2).name(), "created_at");
    assert_eq!(ta_orders_schema.field(3).name(), "customer_name");

    // Verify ta_users is registered
    assert!(registry.contains("ta_users"));
    let ta_users_schema = registry.get("ta_users").unwrap();
    assert_eq!(ta_users_schema.fields().len(), 4);
    assert_eq!(ta_users_schema.field(0).name(), "id");
    assert_eq!(ta_users_schema.field(1).name(), "email");
    assert_eq!(ta_users_schema.field(2).name(), "name");
    assert_eq!(ta_users_schema.field(3).name(), "created_at");
}

#[test]
fn test_register_defaults_includes_ta_tables() {
    let registry = SchemaRegistry::new();

    registry.register_defaults();

    // register_defaults() should call register_ta_tables()
    assert!(registry.contains("ta_orders"));
    assert!(registry.contains("ta_users"));
    assert!(registry.contains("va_orders"));
    assert!(registry.contains("va_users"));
}

#[test]
fn test_infer_schema_from_row_boolean() {
    use std::collections::HashMap;

    let mut row = HashMap::new();
    row.insert("active".to_string(), serde_json::json!(true));

    let schema = infer_schema_from_row("test_view", &row).unwrap();
    assert_eq!(schema.fields().len(), 1);
    assert_eq!(schema.field(0).name(), "active");
    assert!(matches!(schema.field(0).data_type(), arrow::datatypes::DataType::Boolean));
}

#[test]
fn test_infer_schema_from_row_numbers() {
    use std::collections::HashMap;

    let mut row = HashMap::new();
    row.insert("count".to_string(), serde_json::json!(42));
    row.insert("price".to_string(), serde_json::json!(99.99));

    let schema = infer_schema_from_row("test_view", &row).unwrap();
    assert_eq!(schema.fields().len(), 2);
}

#[test]
fn test_infer_schema_from_row_strings() {
    use std::collections::HashMap;

    let mut row = HashMap::new();
    row.insert("name".to_string(), serde_json::json!("John"));
    row.insert("email".to_string(), serde_json::json!("john@example.com"));

    let schema = infer_schema_from_row("test_view", &row).unwrap();
    assert_eq!(schema.fields().len(), 2);
    for field in schema.fields() {
        assert!(matches!(field.data_type(), arrow::datatypes::DataType::Utf8));
    }
}

#[test]
fn test_infer_schema_from_row_nullable() {
    use std::collections::HashMap;

    let mut row = HashMap::new();
    row.insert("optional_field".to_string(), serde_json::json!(null));

    let schema = infer_schema_from_row("test_view", &row).unwrap();
    let field = schema.field(0);
    assert!(field.is_nullable());
}

#[test]
fn test_infer_schema_from_empty_row() {
    use std::collections::HashMap;

    let row = HashMap::new();
    let result = infer_schema_from_row("test_view", &row);
    assert!(
        matches!(result, Err(ArrowFlightError::SchemaNotFound(_))),
        "expected SchemaNotFound error, got: {result:?}"
    );
}

#[test]
fn test_schema_versioning() {
    let registry = SchemaRegistry::new();

    let schema1 = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

    registry.register("va_test", schema1);
    let (version1, _created_at1) = registry.get_version_info("va_test").unwrap();
    assert_eq!(version1, 0);

    // Update schema (version increments)
    let schema2 = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("name", DataType::Utf8, false),
    ]));

    registry.register("va_test", schema2);
    let (version2, _created_at2) = registry.get_version_info("va_test").unwrap();
    assert_eq!(version2, 1);
    assert!(version2 > version1);
}

#[test]
fn test_get_all_versions() {
    let registry = SchemaRegistry::new();

    let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

    registry.register("va_test1", schema.clone());
    registry.register("va_test2", schema.clone());
    registry.register("va_test3", schema);

    let versions = registry.get_all_versions();
    assert_eq!(versions.len(), 3);

    let names: Vec<String> = versions.iter().map(|(name, _, _)| name.clone()).collect();
    assert!(names.contains(&"va_test1".to_string()));
    assert!(names.contains(&"va_test2".to_string()));
    assert!(names.contains(&"va_test3".to_string()));
}

#[test]
fn test_schema_atomic_update() {
    let registry = SchemaRegistry::new();

    let schema1 = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

    registry.register("va_test", schema1.clone());
    let retrieved1 = registry.get("va_test").unwrap();

    // Verify we got the same Arc (not a copy)
    assert!(Arc::ptr_eq(&retrieved1, &schema1));

    // Update with new schema
    let schema2 = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("name", DataType::Utf8, false),
    ]));

    registry.register("va_test", schema2.clone());
    let retrieved2 = registry.get("va_test").unwrap();

    // Old reference still points to old schema
    assert!(Arc::ptr_eq(&retrieved1, &schema1));
    assert!(!Arc::ptr_eq(&retrieved1, &retrieved2));

    // New reference points to new schema
    assert!(Arc::ptr_eq(&retrieved2, &schema2));
    assert_eq!(retrieved2.fields().len(), 2);
}

// --- Additional SchemaRegistry tests ---

#[test]
fn test_registry_new_is_empty() {
    let registry = SchemaRegistry::new();
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);
}

#[test]
fn test_registry_default_is_equivalent_to_new() {
    let a = SchemaRegistry::new();
    let b = SchemaRegistry::default();
    assert_eq!(a.len(), b.len());
}

#[test]
fn test_remove_nonexistent_returns_none() {
    let registry = SchemaRegistry::new();
    let removed = registry.remove("va_does_not_exist");
    assert!(removed.is_none());
}

#[test]
fn test_get_version_info_nonexistent_returns_error() {
    let registry = SchemaRegistry::new();
    let result = registry.get_version_info("va_does_not_exist");
    assert!(
        matches!(result, Err(ArrowFlightError::SchemaNotFound(_))),
        "expected SchemaNotFound error, got: {result:?}"
    );
}

#[test]
fn test_schema_not_found_error_message_contains_view_name() {
    let registry = SchemaRegistry::new();
    let err = registry.get("va_my_view").unwrap_err();
    assert!(err.to_string().contains("va_my_view"));
}

#[test]
fn test_version_counter_monotonically_increases_across_views() {
    let registry = SchemaRegistry::new();
    let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

    registry.register("va_a", schema.clone());
    let (v_a, _) = registry.get_version_info("va_a").unwrap();

    registry.register("va_b", schema.clone());
    let (v_b, _) = registry.get_version_info("va_b").unwrap();

    registry.register("va_c", schema);
    let (v_c, _) = registry.get_version_info("va_c").unwrap();

    assert!(v_b > v_a, "v_b ({v_b}) should be > v_a ({v_a})");
    assert!(v_c > v_b, "v_c ({v_c}) should be > v_b ({v_b})");
}

#[test]
fn test_clear_then_reregister_works() {
    let registry = SchemaRegistry::new();
    let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

    registry.register("va_test", schema.clone());
    assert_eq!(registry.len(), 1);

    registry.clear();
    assert!(registry.is_empty());

    // Re-registering after clear should work
    registry.register("va_test", schema);
    assert_eq!(registry.len(), 1);
    assert!(registry.contains("va_test"));
}

#[test]
fn test_get_all_versions_empty_registry() {
    let registry = SchemaRegistry::new();
    let versions = registry.get_all_versions();
    assert!(versions.is_empty());
}

#[test]
fn test_schema_registry_contains_after_register_defaults() {
    let registry = SchemaRegistry::new();
    registry.register_defaults();
    // All four default views should be present
    assert!(registry.contains("va_orders"));
    assert!(registry.contains("va_users"));
    assert!(registry.contains("ta_orders"));
    assert!(registry.contains("ta_users"));
}
