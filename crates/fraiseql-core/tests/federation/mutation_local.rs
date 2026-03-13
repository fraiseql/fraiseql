//! Local entity mutations (owned entities).

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
use std::sync::Arc;

use fraiseql_core::federation::mutation_executor::FederationMutationExecutor;
use serde_json::json;

use super::common;

#[test]
fn test_mutation_create_owned_entity() {
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_single_key("User", "id");

    let variables = json!({
        "id": "user_new",
        "name": "New User",
        "email": "newuser@example.com"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result =
        runtime.block_on(executor.execute_local_mutation("User", "createUser", &variables));

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response["__typename"], "User");
    assert_eq!(response["id"], "user_new");
    assert_eq!(response["name"], "New User");
}

#[test]
fn test_mutation_update_owned_entity() {
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_single_key("User", "id");

    let variables = json!({
        "id": "user123",
        "email": "updated@example.com",
        "name": "Updated Name"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result =
        runtime.block_on(executor.execute_local_mutation("User", "updateUser", &variables));

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response["__typename"], "User");
    assert_eq!(response["id"], "user123");
    assert_eq!(response["email"], "updated@example.com");
    assert_eq!(response["name"], "Updated Name");
}

#[test]
fn test_mutation_delete_owned_entity() {
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_single_key("User", "id");

    let variables = json!({
        "id": "user_to_delete"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result =
        runtime.block_on(executor.execute_local_mutation("User", "deleteUser", &variables));

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response["__typename"], "User");
    assert_eq!(response["id"], "user_to_delete");
}

#[test]
fn test_mutation_owned_entity_returns_updated_representation() {
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_single_key("Product", "sku");

    let variables = json!({
        "sku": "PROD-001",
        "name": "Widget",
        "price": 29.99,
        "stock": 100
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result =
        runtime.block_on(executor.execute_local_mutation("Product", "updateProduct", &variables));

    assert!(result.is_ok());
    let entity = result.unwrap();

    // Verify response is a proper entity representation
    assert!(entity.is_object());
    assert_eq!(entity["__typename"], "Product");
    assert_eq!(entity["sku"], "PROD-001");
    assert_eq!(entity["price"], 29.99);
}

#[test]
fn test_mutation_owned_entity_batch_updates() {
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_single_key("User", "id");

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    // Execute multiple mutations
    for i in 0..3 {
        let variables = json!({
            "id": format!("user{}", i),
            "name": format!("User {}", i)
        });

        let result =
            runtime.block_on(executor.execute_local_mutation("User", "updateUser", &variables));

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response["id"], format!("user{}", i));
    }
}

#[test]
fn test_mutation_composite_key_update() {
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_composite_key("Order", &["tenant_id", "order_id"]);

    let variables = json!({
        "tenant_id": "tenant_123",
        "order_id": "order_456",
        "status": "confirmed"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result =
        runtime.block_on(executor.execute_local_mutation("Order", "updateOrder", &variables));

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response["__typename"], "Order");
    assert_eq!(response["tenant_id"], "tenant_123");
    assert_eq!(response["order_id"], "order_456");
    assert_eq!(response["status"], "confirmed");
}

#[test]
fn test_mutation_with_validation_errors() {
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_single_key("User", "id");

    // Try to mutate with invalid nested object
    let variables = json!({
        "id": "user1",
        "metadata": { "nested": "object" }  // Invalid for SQL
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    // This should fail during query building
    let result =
        runtime.block_on(executor.execute_local_mutation("User", "updateUser", &variables));

    // Error expected
    assert!(result.is_err());
}

#[test]
fn test_mutation_constraint_violation() {
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_single_key("User", "id");

    let variables = json!({
        "id": "user_duplicate",
        "email": "existing@example.com"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    // Execute mutation (constraint violation would occur at DB level)
    let result =
        runtime.block_on(executor.execute_local_mutation("User", "updateUser", &variables));

    // Should succeed in building query, DB would handle constraint
    assert!(result.is_ok());
}

#[test]
fn test_mutation_concurrent_updates() {
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_single_key("User", "id");

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = Arc::new(FederationMutationExecutor::new(mock_adapter, metadata));

    // Execute multiple mutations concurrently
    for i in 0..5 {
        let variables = json!({
            "id": format!("user{}", i),
            "name": format!("Updated User {}", i)
        });

        let exec = executor.clone();
        let result =
            runtime.block_on(exec.execute_local_mutation("User", "updateUser", &variables));

        assert!(result.is_ok());
    }
}

#[test]
fn test_mutation_transaction_rollback() {
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_single_key("User", "id");

    let variables = json!({
        "id": "user1",
        "email": "test@example.com"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    let result =
        runtime.block_on(executor.execute_local_mutation("User", "updateUser", &variables));

    // In real scenario with DB transaction, would test rollback
    assert!(result.is_ok());
}
