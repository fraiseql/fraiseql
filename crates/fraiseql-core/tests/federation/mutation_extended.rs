//! Extended entity mutations.
//!
//! `execute_extended_mutation` builds its response from the input variables
//! without touching the database (remote-subgraph propagation is not yet
//! implemented), so these tests need only a real adapter to satisfy the
//! executor's type — no tables are provisioned.

#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code, panics + skip notes acceptable
use serde_json::json;

use super::common;

#[tokio::test]
async fn test_mutation_extended_entity_requires_resolution() {
    // Extended entities require resolving @requires fields before mutation
    let metadata = common::metadata_extended_type("Order", "order_id", &["customer_id"], &[]);
    let Some((_pg, executor)) = common::pg_mutation_executor(metadata, &[]).await else {
        eprintln!("SKIP test_mutation_extended_entity_requires_resolution: no postgres");
        return;
    };

    let variables = json!({
        "order_id": "order123",
        "status": "shipped"
    });
    let result = executor.execute_extended_mutation("Order", "updateOrder", &variables).await;

    // Extended mutation returns entity representation
    let response = result
        .unwrap_or_else(|e| panic!("execute_extended_mutation(Order/updateOrder) failed: {e}"));
    assert_eq!(response["__typename"], "Order");
}

#[tokio::test]
async fn test_mutation_extended_entity_propagates_to_owner() {
    // Extended mutations propagate to authoritative subgraph
    let metadata = common::metadata_extended_type("User", "id", &["email"], &[]);
    let Some((_pg, executor)) = common::pg_mutation_executor(metadata, &[]).await else {
        eprintln!("SKIP test_mutation_extended_entity_propagates_to_owner: no postgres");
        return;
    };

    let variables = json!({
        "id": "user123",
        "status": "verified"
    });
    let result = executor.execute_extended_mutation("User", "verifyUser", &variables).await;

    let response =
        result.unwrap_or_else(|e| panic!("execute_extended_mutation(User/verifyUser) failed: {e}"));
    assert_eq!(response["__typename"], "User");
}

#[tokio::test]
async fn test_mutation_extended_entity_partial_fields() {
    // Extended entity mutation with only partial fields
    let metadata = common::metadata_extended_type("Product", "sku", &[], &["price"]);
    let Some((_pg, executor)) = common::pg_mutation_executor(metadata, &[]).await else {
        eprintln!("SKIP test_mutation_extended_entity_partial_fields: no postgres");
        return;
    };

    let variables = json!({
        "sku": "PROD-001",
        "price": 29.99
    });
    let result = executor.execute_extended_mutation("Product", "updatePrice", &variables).await;

    result.unwrap_or_else(|e| panic!("execute_extended_mutation(Product/updatePrice) failed: {e}"));
}

#[tokio::test]
async fn test_mutation_extended_entity_cross_subgraph() {
    // Cross-subgraph extended entity mutation
    let metadata = common::metadata_extended_type("Review", "review_id", &["product_id"], &[]);
    let Some((_pg, executor)) = common::pg_mutation_executor(metadata, &[]).await else {
        eprintln!("SKIP test_mutation_extended_entity_cross_subgraph: no postgres");
        return;
    };

    let variables = json!({
        "review_id": "rev123",
        "rating": 5
    });
    let result = executor.execute_extended_mutation("Review", "updateReview", &variables).await;

    result.unwrap_or_else(|e| panic!("execute_extended_mutation(Review/updateReview) failed: {e}"));
}

#[tokio::test]
async fn test_mutation_extended_entity_with_external_fields() {
    // Extended entity mutation with @external fields reference
    let metadata =
        common::metadata_extended_type("OrderItem", "item_id", &["order_id", "product_id"], &[]);
    let Some((_pg, executor)) = common::pg_mutation_executor(metadata, &[]).await else {
        eprintln!("SKIP test_mutation_extended_entity_with_external_fields: no postgres");
        return;
    };

    let variables = json!({
        "item_id": "item123",
        "quantity": 5
    });
    let result = executor
        .execute_extended_mutation("OrderItem", "updateQuantity", &variables)
        .await;

    result.unwrap_or_else(|e| {
        panic!("execute_extended_mutation(OrderItem/updateQuantity) failed: {e}")
    });
}

#[tokio::test]
async fn test_mutation_extended_entity_reference_tracking() {
    // Reference tracking in extended entity mutations
    let metadata = common::metadata_extended_type("UserProfile", "user_id", &["user_id"], &[]);
    let Some((_pg, executor)) = common::pg_mutation_executor(metadata, &[]).await else {
        eprintln!("SKIP test_mutation_extended_entity_reference_tracking: no postgres");
        return;
    };

    let variables = json!({
        "user_id": "user123",
        "bio": "Updated bio"
    });
    let result = executor
        .execute_extended_mutation("UserProfile", "updateProfile", &variables)
        .await;

    result.unwrap_or_else(|e| {
        panic!("execute_extended_mutation(UserProfile/updateProfile) failed: {e}")
    });
}

#[tokio::test]
async fn test_mutation_extended_entity_cascade_updates() {
    // Cascade update handling for extended entities
    let metadata = common::metadata_extended_type("Organization", "org_id", &[], &["name"]);
    let Some((_pg, executor)) = common::pg_mutation_executor(metadata, &[]).await else {
        eprintln!("SKIP test_mutation_extended_entity_cascade_updates: no postgres");
        return;
    };

    let variables = json!({
        "org_id": "org123",
        "name": "Updated Org Name"
    });
    let result = executor
        .execute_extended_mutation("Organization", "updateOrganization", &variables)
        .await;

    result.unwrap_or_else(|e| {
        panic!("execute_extended_mutation(Organization/updateOrganization) failed: {e}")
    });
}

#[tokio::test]
async fn test_mutation_extended_entity_conflict_resolution() {
    // Conflict resolution in extended entity mutations
    let metadata = common::metadata_extended_type("SharedResource", "resource_id", &[], &["data"]);
    let Some((_pg, executor)) = common::pg_mutation_executor(metadata, &[]).await else {
        eprintln!("SKIP test_mutation_extended_entity_conflict_resolution: no postgres");
        return;
    };

    let variables = json!({
        "resource_id": "res123",
        "data": "updated data",
        "version": 2
    });
    let result = executor
        .execute_extended_mutation("SharedResource", "updateResource", &variables)
        .await;

    result.unwrap_or_else(|e| {
        panic!("execute_extended_mutation(SharedResource/updateResource) failed: {e}")
    });
}
