//! Cross-database federation and type coercion tests.
//!
//! Split from `federation_database_integration.rs`.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
use std::{collections::HashMap, sync::Arc};

use fraiseql_core::federation::{
    database_resolver::DatabaseEntityResolver, selection_parser::FieldSelection,
    types::EntityRepresentation,
};
use serde_json::json;

use super::common;

// ============================================================================
// Cross-Database Federation
// ============================================================================

#[test]
fn test_cross_database_postgres_to_mysql() {
    let mut user = HashMap::new();
    user.insert("id".to_string(), json!("user123"));
    user.insert("username".to_string(), json!("alice"));

    let mock_adapter = Arc::new(
        common::MockDatabaseAdapter::new().with_table_data("user".to_string(), vec![user]),
    );

    let metadata = common::metadata_single_key("User", "id");

    let mut rep_keys = HashMap::new();
    rep_keys.insert("id".to_string(), json!("user123"));
    let mut rep_all = HashMap::new();
    rep_all.insert("id".to_string(), json!("user123"));

    let representation = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "username".to_string(),
    ]);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let resolver = DatabaseEntityResolver::new(mock_adapter, metadata);
    let result =
        runtime.block_on(resolver.resolve_entities_from_db("User", &[representation], &selection));

    let entities = result.unwrap_or_else(|e| panic!("resolve_entities_from_db (postgres→mysql) failed: {e}"));
    assert_eq!(entities.len(), 1);
    assert!(entities[0].is_some());
    assert_eq!(entities[0].as_ref().unwrap()["username"], "alice");
}

#[test]
fn test_cross_database_postgres_to_sqlserver() {
    let mut product = HashMap::new();
    product.insert("product_id".to_string(), json!("prod123"));
    product.insert("product_name".to_string(), json!("Widget"));
    product.insert("price".to_string(), json!(29.99));

    let mock_adapter = Arc::new(
        common::MockDatabaseAdapter::new().with_table_data("product".to_string(), vec![product]),
    );

    let metadata = common::metadata_single_key("Product", "product_id");

    let mut rep_keys = HashMap::new();
    rep_keys.insert("product_id".to_string(), json!("prod123"));
    let mut rep_all = HashMap::new();
    rep_all.insert("product_id".to_string(), json!("prod123"));

    let representation = EntityRepresentation {
        typename:   "Product".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "product_id".to_string(),
        "product_name".to_string(),
        "price".to_string(),
    ]);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let resolver = DatabaseEntityResolver::new(mock_adapter, metadata);
    let result = runtime.block_on(resolver.resolve_entities_from_db(
        "Product",
        &[representation],
        &selection,
    ));

    let entities = result.unwrap_or_else(|e| panic!("resolve_entities_from_db (postgres→sqlserver) failed: {e}"));
    assert_eq!(entities.len(), 1);
    assert!(entities[0].is_some());
    assert_eq!(entities[0].as_ref().unwrap()["product_name"], "Widget");
}

#[test]
fn test_cross_database_type_coercion_numeric() {
    let mut order = HashMap::new();
    order.insert("order_id".to_string(), json!("order123"));
    order.insert("amount".to_string(), json!(100));
    order.insert("discount_rate".to_string(), json!(0.15));

    let mock_adapter = Arc::new(
        common::MockDatabaseAdapter::new().with_table_data("order".to_string(), vec![order]),
    );

    let metadata = common::metadata_single_key("Order", "order_id");

    let mut rep_keys = HashMap::new();
    rep_keys.insert("order_id".to_string(), json!("order123"));
    let mut rep_all = HashMap::new();
    rep_all.insert("order_id".to_string(), json!("order123"));

    let representation = EntityRepresentation {
        typename:   "Order".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "order_id".to_string(),
        "amount".to_string(),
        "discount_rate".to_string(),
    ]);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let resolver = DatabaseEntityResolver::new(mock_adapter, metadata);
    let result =
        runtime.block_on(resolver.resolve_entities_from_db("Order", &[representation], &selection));

    let entities = result.unwrap_or_else(|e| panic!("resolve_entities_from_db (numeric coercion) failed: {e}"));
    assert_eq!(entities.len(), 1);
    assert!(entities[0].is_some());

    let entity = entities[0].as_ref().unwrap();
    assert_eq!(entity["amount"], 100);
    assert_eq!(entity["discount_rate"], 0.15);
}

#[test]
fn test_cross_database_type_coercion_string() {
    let mut customer = HashMap::new();
    customer.insert("customer_id".to_string(), json!("cust123"));
    customer.insert("email".to_string(), json!("test@example.com"));
    customer.insert("phone".to_string(), json!("+1-555-1234"));

    let mock_adapter = Arc::new(
        common::MockDatabaseAdapter::new().with_table_data("customer".to_string(), vec![customer]),
    );

    let metadata = common::metadata_single_key("Customer", "customer_id");

    let mut rep_keys = HashMap::new();
    rep_keys.insert("customer_id".to_string(), json!("cust123"));
    let mut rep_all = HashMap::new();
    rep_all.insert("customer_id".to_string(), json!("cust123"));

    let representation = EntityRepresentation {
        typename:   "Customer".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "customer_id".to_string(),
        "email".to_string(),
        "phone".to_string(),
    ]);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let resolver = DatabaseEntityResolver::new(mock_adapter, metadata);
    let result = runtime.block_on(resolver.resolve_entities_from_db(
        "Customer",
        &[representation],
        &selection,
    ));

    let entities = result.unwrap_or_else(|e| panic!("resolve_entities_from_db (string coercion) failed: {e}"));
    assert_eq!(entities.len(), 1);
    assert!(entities[0].is_some());

    let entity = entities[0].as_ref().unwrap();
    assert_eq!(entity["email"], "test@example.com");
    assert_eq!(entity["phone"], "+1-555-1234");
}

#[test]
fn test_cross_database_type_coercion_datetime() {
    let mut event = HashMap::new();
    event.insert("event_id".to_string(), json!("evt123"));
    event.insert("event_date".to_string(), json!("2024-01-15T10:30:00Z"));
    event.insert("created_at".to_string(), json!("2024-01-15T00:00:00Z"));

    let mock_adapter = Arc::new(
        common::MockDatabaseAdapter::new().with_table_data("event".to_string(), vec![event]),
    );

    let metadata = common::metadata_single_key("Event", "event_id");

    let mut rep_keys = HashMap::new();
    rep_keys.insert("event_id".to_string(), json!("evt123"));
    let mut rep_all = HashMap::new();
    rep_all.insert("event_id".to_string(), json!("evt123"));

    let representation = EntityRepresentation {
        typename:   "Event".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "event_id".to_string(),
        "event_date".to_string(),
        "created_at".to_string(),
    ]);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let resolver = DatabaseEntityResolver::new(mock_adapter, metadata);
    let result =
        runtime.block_on(resolver.resolve_entities_from_db("Event", &[representation], &selection));

    let entities = result.unwrap_or_else(|e| panic!("resolve_entities_from_db (datetime coercion) failed: {e}"));
    assert_eq!(entities.len(), 1);
    assert!(entities[0].is_some());

    let entity = entities[0].as_ref().unwrap();
    assert_eq!(entity["event_date"], "2024-01-15T10:30:00Z");
    assert_eq!(entity["created_at"], "2024-01-15T00:00:00Z");
}
