//! Docker Compose Federation Tests - Entity Resolution & Error Handling
//!
//! Tests validate entity resolution across subgraphs and invalid query error handling.

use super::common::*;

// ============================================================================
// Entity Resolution Tests
// ============================================================================

#[tokio::test]
async fn test_user_entity_resolution() {
    if std::env::var("FEDERATION_TESTS").is_err() {
        eprintln!("Skipping: FEDERATION_TESTS not set");
        return;
    }
    // Query users to get IDs
    let users_response = graphql_query(USERS_SUBGRAPH_URL, "query { users(limit: 1) { id } }")
        .await
        .expect("Initial query should succeed");

    let user_id = extract_data(&users_response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .and_then(|arr| arr.first())
        .and_then(|user| user.get("id"))
        .and_then(|id| id.as_str())
        .expect("Should extract first user ID");

    // Query user by ID through federation
    let query = format!(r#"query {{ user(id: "{}") {{ id identifier }} }}"#, user_id);

    let response = graphql_query(ORDERS_SUBGRAPH_URL, &query)
        .await
        .expect("Entity resolution query should succeed");

    assert!(!has_errors(&response), "Entity resolution should not have errors");

    let data = extract_data(&response)
        .and_then(|d| d.get("user"))
        .expect("Should return resolved user entity");

    assert_eq!(
        data.get("id").and_then(|v| v.as_str()),
        Some(user_id),
        "Resolved user should have correct ID"
    );

    println!("✓ User entity resolution succeeded");
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_gateway_invalid_query_error_handling() {
    if std::env::var("FEDERATION_TESTS").is_err() {
        eprintln!("Skipping: FEDERATION_TESTS not set");
        return;
    }
    let response = graphql_query(APOLLO_GATEWAY_URL, "query { invalidField { subfield } }")
        .await
        .expect("Request should complete even with invalid query");

    assert!(has_errors(&response), "Invalid query should return errors");

    let errors = response
        .get("errors")
        .and_then(|e| e.as_array())
        .expect("Should have errors array");

    assert!(!errors.is_empty(), "Should have at least one error");
    println!("✓ Invalid query error handling works");
}
