//! Docker Compose Federation Integration Tests - Single Subgraph & Basic Gateway Queries
//!
//! Tests validate direct subgraph queries and basic gateway routing.

use super::common::*;

// ============================================================================
// Single Subgraph Query Tests
// ============================================================================

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
async fn test_users_subgraph_query() {
    let response = graphql_query(USERS_SUBGRAPH_URL, "query { users { id identifier } }")
        .await
        .expect("Query should succeed");

    assert!(
        !has_errors(&response),
        "Query should not have errors: {:?}",
        response.get("errors")
    );

    let data = extract_data(&response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .expect("Should return array of users");

    assert!(!data.is_empty(), "Should have at least one user");

    // Verify user has id and identifier fields
    let first_user = &data[0];
    assert!(first_user.get("id").is_some(), "User should have id");
    assert!(first_user.get("identifier").is_some(), "User should have identifier");

    println!("✓ Users query succeeded with {} results", data.len());
}

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
async fn test_orders_subgraph_query() {
    let response = graphql_query(ORDERS_SUBGRAPH_URL, "query { orders { id status total } }")
        .await
        .expect("Query should succeed");

    assert!(
        !has_errors(&response),
        "Query should not have errors: {:?}",
        response.get("errors")
    );

    let data = extract_data(&response)
        .and_then(|d| d.get("orders"))
        .and_then(|o| o.as_array())
        .expect("Should return array of orders");

    assert!(!data.is_empty(), "Should have at least one order");
    println!("✓ Orders query succeeded with {} results", data.len());
}

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
async fn test_products_subgraph_query() {
    let response = graphql_query(PRODUCTS_SUBGRAPH_URL, "query { products { id name price } }")
        .await
        .expect("Query should succeed");

    assert!(
        !has_errors(&response),
        "Query should not have errors: {:?}",
        response.get("errors")
    );

    let data = extract_data(&response)
        .and_then(|d| d.get("products"))
        .and_then(|p| p.as_array())
        .expect("Should return array of products");

    assert!(!data.is_empty(), "Should have at least one product");
    println!("✓ Products query succeeded with {} results", data.len());
}

// ============================================================================
// Federated Query Tests (via Apollo Router Gateway)
// ============================================================================

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
async fn test_gateway_simple_query() {
    let response = graphql_query(APOLLO_GATEWAY_URL, "query { users { id identifier } }")
        .await
        .expect("Query should succeed");

    assert!(
        !has_errors(&response),
        "Query should not have errors: {:?}",
        response.get("errors")
    );

    let data = extract_data(&response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .expect("Should return array of users");

    assert!(!data.is_empty(), "Should have at least one user");
    println!("✓ Gateway simple query succeeded");
}

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
async fn test_two_subgraph_direct_subgraph_queries() {
    setup_federation_tests().await.expect("Setup should succeed");

    // Test 1: Query users directly from users subgraph
    println!("\n--- Test 1: Query users directly ---");
    let users_response =
        graphql_query(USERS_SUBGRAPH_URL, r"query { users { id identifier email } }")
            .await
            .expect("Users query should succeed");

    assert!(
        !has_errors(&users_response),
        "Users query should not have errors: {}",
        get_error_messages(&users_response)
    );

    let users = extract_data(&users_response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .expect("Should return users array");

    assert!(!users.is_empty(), "Should have users in database");
    println!("✓ Found {} users from users subgraph", users.len());

    // Test 2: Query orders directly from orders subgraph
    println!("\n--- Test 2: Query orders directly ---");
    let orders_response =
        graphql_query(ORDERS_SUBGRAPH_URL, r"query { orders { id status total } }")
            .await
            .expect("Orders query should succeed");

    assert!(
        !has_errors(&orders_response),
        "Orders query should not have errors: {}",
        get_error_messages(&orders_response)
    );

    let orders = extract_data(&orders_response)
        .and_then(|d| d.get("orders"))
        .and_then(|o| o.as_array())
        .expect("Should return orders array");

    assert!(!orders.is_empty(), "Should have orders in database");
    println!("✓ Found {} orders from orders subgraph", orders.len());
}
