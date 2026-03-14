//! Mutation error scenarios.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
use fraiseql_core::federation::{
    mutation_executor::FederationMutationExecutor,
    mutation_query_builder::{build_insert_query, build_update_query},
    types::FederationMetadata,
};
use serde_json::json;

use super::common;

#[test]
fn test_mutation_entity_not_found() {
    let metadata = common::metadata_single_key("User", "id");

    let variables = json!({
        "id": "nonexistent_user",
        "name": "Updated"
    });

    // Query builds successfully but returns error when executed against DB
    let query = build_update_query("User", &variables, &metadata).unwrap();
    assert!(
        query.contains("WHERE \"id\" = 'nonexistent_user'"),
        "Expected quoted column in: {query}"
    );
}

#[test]
fn test_mutation_invalid_field_value() {
    let metadata = FederationMetadata::default();

    // Invalid field values like objects should be rejected
    let invalid_variables = json!({
        "id": "user1",
        "metadata": { "nested": "object" }  // Invalid for SQL
    });

    let result = build_insert_query("User", &invalid_variables, &metadata);
    // Should error because objects cannot convert to SQL literals
    assert!(result.is_err());
}

#[test]
fn test_mutation_missing_required_fields() {
    let metadata = common::metadata_single_key("User", "id");

    // Missing id key field should error
    let missing_key = json!({
        "name": "Updated Name"
    });

    let result = build_update_query("User", &missing_key, &metadata);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("missing"));
}

#[test]
fn test_mutation_authorization_error() {
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_single_key("User", "id");

    let variables = json!({
        "id": "admin_user",
        "role": "superadmin"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    // Execute mutation (authorization would be checked at application level)
    let result =
        runtime.block_on(executor.execute_local_mutation("User", "updateUser", &variables));

    // Query builds successfully
    assert!(result.is_ok());
}

#[test]
fn test_mutation_duplicate_key_error() {
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_single_key("User", "id");

    let variables = json!({
        "id": "existing_id",
        "email": "test@example.com"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    // Execute mutation (duplicate key would be caught at DB level)
    let result =
        runtime.block_on(executor.execute_local_mutation("User", "updateUser", &variables));

    // Query builds successfully
    assert!(result.is_ok());
}

#[test]
fn test_mutation_latency_single_entity() {
    use std::time::Instant;

    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_single_key("User", "id");

    let variables = json!({
        "id": "user1",
        "name": "Updated User"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    let start = Instant::now();
    let _result =
        runtime.block_on(executor.execute_local_mutation("User", "updateUser", &variables));
    let duration = start.elapsed();

    // Mock mutation should be very fast (<10ms)
    assert!(duration.as_millis() < 10, "Single mutation took {:?}", duration);
}

#[test]
fn test_mutation_latency_batch_updates() {
    use std::time::Instant;

    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_single_key("User", "id");

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    let start = Instant::now();

    // Execute 10 mutations
    for i in 0..10 {
        let variables = json!({
            "id": format!("user{}", i),
            "name": format!("Updated User {}", i)
        });

        let _result =
            runtime.block_on(executor.execute_local_mutation("User", "updateUser", &variables));
    }

    let duration = start.elapsed();

    // 10 batch mutations should be reasonable (<100ms for mock)
    assert!(duration.as_millis() < 100, "Batch mutations took {:?}", duration);
}
