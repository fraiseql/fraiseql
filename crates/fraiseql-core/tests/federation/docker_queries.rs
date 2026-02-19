//! Docker Compose Federation Integration Tests - Single Subgraph & Basic Gateway Queries
//!
//! Tests validate direct subgraph queries and basic gateway routing.

use std::time::Duration;

use serde_json::{Value, json};

const APOLLO_GATEWAY_URL: &str = "http://localhost:4000/graphql";
const USERS_SUBGRAPH_URL: &str = "http://localhost:4001/graphql";
const ORDERS_SUBGRAPH_URL: &str = "http://localhost:4002/graphql";
const PRODUCTS_SUBGRAPH_URL: &str = "http://localhost:4003/graphql";

/// Wait for a service to be ready with health check
async fn wait_for_service(url: &str, max_retries: u32) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let mut retries = 0;

    loop {
        match client
            .post(url)
            .json(&json!({ "query": "{ __typename }" }))
            .timeout(Duration::from_secs(5))
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                println!("✓ Service ready: {}", url);
                return Ok(());
            },
            Ok(response) => {
                println!("✗ Service {} returned status: {}", url, response.status());
            },
            Err(e) => {
                println!("✗ Service {} connection failed: {}", url, e);
            },
        }

        retries += 1;
        if retries >= max_retries {
            return Err(format!(
                "Service {} failed to become ready after {} retries",
                url, max_retries
            )
            .into());
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Execute a GraphQL query against a service
async fn graphql_query(url: &str, query: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .json(&json!({ "query": query }))
        .timeout(Duration::from_secs(10))
        .send()
        .await?;

    let body: Value = response.json().await?;
    Ok(body)
}

/// Test helper to extract data from GraphQL response
fn extract_data(response: &Value) -> Option<&Value> {
    response.get("data")
}

/// Test helper to check for GraphQL errors
fn has_errors(response: &Value) -> bool {
    response.get("errors").is_some()
}

/// Get error message from GraphQL response
#[allow(dead_code)]
fn get_error_messages(response: &Value) -> String {
    response
        .get("errors")
        .and_then(|e| e.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|err| err.get("message")?.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        })
        .unwrap_or_else(|| "Unknown error".to_string())
}

/// Setup test fixtures - ensures services are ready
#[allow(dead_code)]
async fn setup_federation_tests() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Setting up 2-subgraph federation tests ===\n");

    // Wait for all services
    println!("Waiting for users subgraph...");
    wait_for_service(USERS_SUBGRAPH_URL, 30).await?;

    println!("Waiting for orders subgraph...");
    wait_for_service(ORDERS_SUBGRAPH_URL, 30).await?;

    println!("Waiting for Apollo Router gateway...");
    wait_for_service(APOLLO_GATEWAY_URL, 30).await?;

    println!("\n✓ All services ready for 2-subgraph federation tests\n");
    Ok(())
}

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
        graphql_query(USERS_SUBGRAPH_URL, r#"query { users { id identifier email } }"#)
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
        graphql_query(ORDERS_SUBGRAPH_URL, r#"query { orders { id status total } }"#)
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
