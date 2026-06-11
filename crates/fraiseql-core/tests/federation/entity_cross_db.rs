//! Entity resolution over varied field shapes against **real PostgreSQL** —
//! string, numeric, and timestamp-string fields.
//!
//! (These were previously named "cross-database" but never touched MySQL/SQL
//! Server; they exercise the resolver's value mapping against a real database.)

#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code (skip notes to stderr)
use fraiseql_core::federation::{
    database_resolver::DatabaseEntityResolver, selection_parser::FieldSelection,
};
use serde_json::json;

use super::common;

#[tokio::test]
async fn test_resolve_entity_with_string_fields() {
    let rows = vec![common::row(&[
        ("id", json!("user123")),
        ("username", json!("alice")),
    ])];
    let Some((_pg, adapter)) =
        common::pg_entity_fixture("user", &["id text", "username text"], &rows).await
    else {
        eprintln!("SKIP test_resolve_entity_with_string_fields: no postgres");
        return;
    };

    let resolver = DatabaseEntityResolver::new(adapter, common::metadata_single_key("User", "id"));
    let entities = resolver
        .resolve_entities_from_db(
            "User",
            &[common::rep("User", &[("id", json!("user123"))])],
            &FieldSelection::new(vec![
                "__typename".to_string(),
                "id".to_string(),
                "username".to_string(),
            ]),
        )
        .await
        .unwrap_or_else(|e| panic!("resolve failed: {e}"));

    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0].as_ref().expect("entity")["username"], "alice");
}

#[tokio::test]
async fn test_resolve_entity_with_mixed_fields() {
    let rows = vec![common::row(&[
        ("product_id", json!("prod123")),
        ("product_name", json!("Widget")),
        ("price", json!(29.99)),
    ])];
    let Some((_pg, adapter)) = common::pg_entity_fixture(
        "product",
        &[
            "product_id text",
            "product_name text",
            "price double precision",
        ],
        &rows,
    )
    .await
    else {
        eprintln!("SKIP test_resolve_entity_with_mixed_fields: no postgres");
        return;
    };

    let resolver =
        DatabaseEntityResolver::new(adapter, common::metadata_single_key("Product", "product_id"));
    let entities = resolver
        .resolve_entities_from_db(
            "Product",
            &[common::rep("Product", &[("product_id", json!("prod123"))])],
            &FieldSelection::new(vec![
                "__typename".to_string(),
                "product_id".to_string(),
                "product_name".to_string(),
                "price".to_string(),
            ]),
        )
        .await
        .unwrap_or_else(|e| panic!("resolve failed: {e}"));

    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0].as_ref().expect("entity")["product_name"], "Widget");
}

#[tokio::test]
async fn test_resolve_numeric_fields() {
    let rows = vec![common::row(&[
        ("order_id", json!("order123")),
        ("amount", json!(100)),
        ("discount_rate", json!(0.15)),
    ])];
    let Some((_pg, adapter)) = common::pg_entity_fixture(
        "order",
        &[
            "order_id text",
            "amount integer",
            "discount_rate double precision",
        ],
        &rows,
    )
    .await
    else {
        eprintln!("SKIP test_resolve_numeric_fields: no postgres");
        return;
    };

    let resolver =
        DatabaseEntityResolver::new(adapter, common::metadata_single_key("Order", "order_id"));
    let entities = resolver
        .resolve_entities_from_db(
            "Order",
            &[common::rep("Order", &[("order_id", json!("order123"))])],
            &FieldSelection::new(vec![
                "__typename".to_string(),
                "order_id".to_string(),
                "amount".to_string(),
                "discount_rate".to_string(),
            ]),
        )
        .await
        .unwrap_or_else(|e| panic!("resolve failed: {e}"));

    assert_eq!(entities.len(), 1);
    let entity = entities[0].as_ref().expect("entity");
    assert_eq!(entity["amount"], 100);
    assert_eq!(entity["discount_rate"], 0.15);
}

#[tokio::test]
async fn test_resolve_string_typed_fields() {
    let rows = vec![common::row(&[
        ("customer_id", json!("cust123")),
        ("email", json!("test@example.com")),
        ("phone", json!("+1-555-1234")),
    ])];
    let Some((_pg, adapter)) = common::pg_entity_fixture(
        "customer",
        &["customer_id text", "email text", "phone text"],
        &rows,
    )
    .await
    else {
        eprintln!("SKIP test_resolve_string_typed_fields: no postgres");
        return;
    };

    let resolver = DatabaseEntityResolver::new(
        adapter,
        common::metadata_single_key("Customer", "customer_id"),
    );
    let entities = resolver
        .resolve_entities_from_db(
            "Customer",
            &[common::rep(
                "Customer",
                &[("customer_id", json!("cust123"))],
            )],
            &FieldSelection::new(vec![
                "__typename".to_string(),
                "customer_id".to_string(),
                "email".to_string(),
                "phone".to_string(),
            ]),
        )
        .await
        .unwrap_or_else(|e| panic!("resolve failed: {e}"));

    assert_eq!(entities.len(), 1);
    let entity = entities[0].as_ref().expect("entity");
    assert_eq!(entity["email"], "test@example.com");
    assert_eq!(entity["phone"], "+1-555-1234");
}

#[tokio::test]
async fn test_resolve_timestamp_string_fields() {
    let rows = vec![common::row(&[
        ("event_id", json!("evt123")),
        ("event_date", json!("2024-01-15T10:30:00Z")),
        ("created_at", json!("2024-01-15T00:00:00Z")),
    ])];
    let Some((_pg, adapter)) = common::pg_entity_fixture(
        "event",
        &["event_id text", "event_date text", "created_at text"],
        &rows,
    )
    .await
    else {
        eprintln!("SKIP test_resolve_timestamp_string_fields: no postgres");
        return;
    };

    let resolver =
        DatabaseEntityResolver::new(adapter, common::metadata_single_key("Event", "event_id"));
    let entities = resolver
        .resolve_entities_from_db(
            "Event",
            &[common::rep("Event", &[("event_id", json!("evt123"))])],
            &FieldSelection::new(vec![
                "__typename".to_string(),
                "event_id".to_string(),
                "event_date".to_string(),
                "created_at".to_string(),
            ]),
        )
        .await
        .unwrap_or_else(|e| panic!("resolve failed: {e}"));

    assert_eq!(entities.len(), 1);
    let entity = entities[0].as_ref().expect("entity");
    assert_eq!(entity["event_date"], "2024-01-15T10:30:00Z");
    assert_eq!(entity["created_at"], "2024-01-15T00:00:00Z");
}
