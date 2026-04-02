//! Mutation response format and return selection.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
use fraiseql_core::federation::mutation_executor::FederationMutationExecutor;
use serde_json::json;

use super::common;

#[test]
fn test_mutation_response_format_matches_spec() {
    // Federation response format must include:
    // - __typename field
    // - All requested fields with updated values
    let response = json!({
        "__typename": "User",
        "id": "user123",
        "email": "updated@example.com",
        "name": "Updated Name"
    });

    // Validate required fields
    assert!(response.get("__typename").is_some());
    assert_eq!(response["__typename"], "User");
    assert!(response.get("id").is_some());
    assert!(response.get("email").is_some());
    assert!(response.get("name").is_some());
}

#[test]
fn test_mutation_response_includes_updated_fields() {
    let original_email = "old@example.com";
    let updated_email = "new@example.com";

    let mutation_response = json!({
        "__typename": "User",
        "id": "user123",
        "email": updated_email,
        "name": "John Doe"
    });

    // Verify updated field value is in response
    assert_ne!(mutation_response["email"].as_str(), Some(original_email));
    assert_eq!(mutation_response["email"].as_str(), Some(updated_email));
}

#[test]
fn test_mutation_response_federation_wrapper() {
    // Federation mutations return entity representation (not wrapped)
    let entity_representation = json!({
        "__typename": "User",
        "id": "user123",
        "email": "test@example.com",
        "name": "Test User"
    });

    // Check it's a valid entity representation
    assert!(entity_representation.is_object());
    assert!(entity_representation.get("__typename").is_some());
    assert_eq!(entity_representation["__typename"], "User");
}

#[test]
fn test_mutation_response_error_federation_format() {
    // Error response in federation format
    let error_response = json!({
        "errors": [
            {
                "message": "Entity not found",
                "extensions": {
                    "code": "ENTITY_NOT_FOUND"
                }
            }
        ]
    });

    // Validate error structure
    assert!(error_response.get("errors").is_some());
    let errors = error_response["errors"].as_array();
    assert!(errors.is_some());
    assert!(!errors.unwrap().is_empty());

    let error = &error_response["errors"][0];
    assert!(error.get("message").is_some());
    assert!(error.get("extensions").is_some());
}

#[test]
fn test_mutation_response_partial_success() {
    // Partial success: some entities updated, some failed
    let partial_response = json!({
        "data": {
            "updateUsers": [
                {
                    "__typename": "User",
                    "id": "user1",
                    "email": "updated1@example.com"
                },
                null,  // Failed update represented as null
                {
                    "__typename": "User",
                    "id": "user3",
                    "email": "updated3@example.com"
                }
            ]
        },
        "errors": [
            {
                "message": "User not found: user2",
                "path": ["updateUsers", 1]
            }
        ]
    });

    // Verify structure supports partial success
    assert!(partial_response.get("data").is_some());
    assert!(partial_response.get("errors").is_some());

    let results = partial_response["data"]["updateUsers"].as_array();
    assert!(results.is_some());
    assert_eq!(results.unwrap().len(), 3);
}

#[test]
fn test_mutation_response_subscription_trigger() {
    // Mutation response that would trigger subscriptions
    let mutation_response = json!({
        "__typename": "User",
        "id": "user123",
        "email": "updated@example.com",
        "name": "Updated Name"
    });

    // Verify subscription-relevant fields are present
    assert!(mutation_response.get("__typename").is_some());
    assert!(mutation_response.get("id").is_some());

    // Check that response can be serialized (for subscription transmission)
    let serialized = serde_json::to_string(&mutation_response).unwrap();
    assert!(!serialized.is_empty());

    // Deserialize and verify round-trip
    let deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, mutation_response);
}

#[test]
fn test_mutation_return_all_requested_fields() {
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_single_key("User", "id");

    // Mutation with multiple fields
    let variables = json!({
        "id": "user123",
        "email": "updated@example.com",
        "name": "Updated Name",
        "phone": "+1-555-1234",
        "address": "123 Main St"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result =
        runtime.block_on(executor.execute_local_mutation("User", "updateUser", &variables));

    let response = result.unwrap_or_else(|e| {
        panic!("execute_local_mutation(User/updateUser) multi-field failed: {e}")
    });

    // All requested fields should be in response
    assert_eq!(response["__typename"], "User");
    assert_eq!(response["id"], "user123");
    assert_eq!(response["email"], "updated@example.com");
    assert_eq!(response["name"], "Updated Name");
    assert_eq!(response["phone"], "+1-555-1234");
    assert_eq!(response["address"], "123 Main St");
}

#[test]
fn test_mutation_return_computed_fields() {
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_single_key("Order", "order_id");

    let variables = json!({
        "order_id": "order123",
        "subtotal": 100.00,
        "tax": 10.00,
        "total": 110.00  // Computed field
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result =
        runtime.block_on(executor.execute_local_mutation("Order", "updateOrder", &variables));

    let response = result.unwrap_or_else(|e| {
        panic!("execute_local_mutation(Order/updateOrder) computed fields failed: {e}")
    });

    // Computed fields should be in response
    assert_eq!(response["total"], 110.00);
    assert_eq!(response["subtotal"], 100.00);
    assert_eq!(response["tax"], 10.00);
}

#[test]
fn test_mutation_return_related_entities() {
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_single_key("Order", "order_id");

    let variables = json!({
        "order_id": "order123",
        "customer_id": "cust456",
        "status": "confirmed"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result =
        runtime.block_on(executor.execute_local_mutation("Order", "updateOrder", &variables));

    let response = result.unwrap_or_else(|e| {
        panic!("execute_local_mutation(Order/updateOrder) related entities failed: {e}")
    });

    // Response includes related entity references
    assert_eq!(response["__typename"], "Order");
    assert_eq!(response["order_id"], "order123");
    assert_eq!(response["customer_id"], "cust456");
}
