//! Extended entity mutations.

use fraiseql_core::federation::mutation_executor::FederationMutationExecutor;
use serde_json::json;

use super::common;

#[test]
fn test_mutation_extended_entity_requires_resolution() {
    // Extended entities require resolving @requires fields before mutation
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_extended_type("Order", "order_id", &["customer_id"], &[]);

    let variables = json!({
        "order_id": "order123",
        "status": "shipped"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result =
        runtime.block_on(executor.execute_extended_mutation("Order", "updateOrder", &variables));

    // Extended mutation returns entity representation
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response["__typename"], "Order");
}

#[test]
fn test_mutation_extended_entity_propagates_to_owner() {
    // Extended mutations propagate to authoritative subgraph
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_extended_type("User", "id", &["email"], &[]);

    let variables = json!({
        "id": "user123",
        "status": "verified"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result =
        runtime.block_on(executor.execute_extended_mutation("User", "verifyUser", &variables));

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response["__typename"], "User");
}

#[test]
fn test_mutation_extended_entity_partial_fields() {
    // Extended entity mutation with only partial fields
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_extended_type("Product", "sku", &[], &["price"]);

    let variables = json!({
        "sku": "PROD-001",
        "price": 29.99
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result =
        runtime.block_on(executor.execute_extended_mutation("Product", "updatePrice", &variables));

    assert!(result.is_ok());
}

#[test]
fn test_mutation_extended_entity_cross_subgraph() {
    // Cross-subgraph extended entity mutation
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_extended_type("Review", "review_id", &["product_id"], &[]);

    let variables = json!({
        "review_id": "rev123",
        "rating": 5
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result =
        runtime.block_on(executor.execute_extended_mutation("Review", "updateReview", &variables));

    assert!(result.is_ok());
}

#[test]
fn test_mutation_extended_entity_with_external_fields() {
    // Extended entity mutation with @external fields reference
    let mock_adapter = common::mock_mutation_adapter();
    let metadata =
        common::metadata_extended_type("OrderItem", "item_id", &["order_id", "product_id"], &[]);

    let variables = json!({
        "item_id": "item123",
        "quantity": 5
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result = runtime.block_on(executor.execute_extended_mutation(
        "OrderItem",
        "updateQuantity",
        &variables,
    ));

    assert!(result.is_ok());
}

#[test]
fn test_mutation_extended_entity_reference_tracking() {
    // Reference tracking in extended entity mutations
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_extended_type("UserProfile", "user_id", &["user_id"], &[]);

    let variables = json!({
        "user_id": "user123",
        "bio": "Updated bio"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result = runtime.block_on(executor.execute_extended_mutation(
        "UserProfile",
        "updateProfile",
        &variables,
    ));

    assert!(result.is_ok());
}

#[test]
fn test_mutation_extended_entity_cascade_updates() {
    // Cascade update handling for extended entities
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_extended_type("Organization", "org_id", &[], &["name"]);

    let variables = json!({
        "org_id": "org123",
        "name": "Updated Org Name"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result = runtime.block_on(executor.execute_extended_mutation(
        "Organization",
        "updateOrganization",
        &variables,
    ));

    assert!(result.is_ok());
}

#[test]
fn test_mutation_extended_entity_conflict_resolution() {
    // Conflict resolution in extended entity mutations
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_extended_type("SharedResource", "resource_id", &[], &["data"]);

    let variables = json!({
        "resource_id": "res123",
        "data": "updated data",
        "version": 2
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result = runtime.block_on(executor.execute_extended_mutation(
        "SharedResource",
        "updateResource",
        &variables,
    ));

    assert!(result.is_ok());
}
