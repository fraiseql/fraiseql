//! Docker Compose Integration Tests for Apollo Federation
//!
//! This test suite verifies the multi-subgraph federation setup using Docker Compose.
//! Tests validate:
//! - Service startup and health checks
//! - Schema composition through Apollo Router
//! - Cross-subgraph entity resolution
//! - Federated query execution
//! - Extended mutations

use serde_json::{json, Value};
use std::time::Duration;

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
            }
            Ok(response) => {
                println!(
                    "✗ Service {} returned status: {}",
                    url,
                    response.status()
                );
            }
            Err(e) => {
                println!("✗ Service {} connection failed: {}", url, e);
            }
        }

        retries += 1;
        if retries >= max_retries {
            return Err(format!("Service {} failed to become ready after {} retries", url, max_retries).into());
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Execute a GraphQL query against a service
async fn graphql_query(
    url: &str,
    query: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
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

// ============================================================================
// Service Health & Composition Tests
// ============================================================================

#[tokio::test]
#[ignore] // Run with: cargo test -- --ignored
async fn test_users_subgraph_health() {
    if let Err(e) = wait_for_service(USERS_SUBGRAPH_URL, 30).await {
        panic!("Users subgraph health check failed: {}", e);
    }
}

#[tokio::test]
#[ignore]
async fn test_orders_subgraph_health() {
    if let Err(e) = wait_for_service(ORDERS_SUBGRAPH_URL, 30).await {
        panic!("Orders subgraph health check failed: {}", e);
    }
}

#[tokio::test]
#[ignore]
async fn test_products_subgraph_health() {
    if let Err(e) = wait_for_service(PRODUCTS_SUBGRAPH_URL, 30).await {
        panic!("Products subgraph health check failed: {}", e);
    }
}

#[tokio::test]
#[ignore]
async fn test_apollo_router_health() {
    if let Err(e) = wait_for_service(APOLLO_GATEWAY_URL, 30).await {
        panic!("Apollo Router health check failed: {}", e);
    }
}

#[tokio::test]
#[ignore]
async fn test_apollo_router_schema_composition() {
    // Wait for gateway to be ready
    wait_for_service(APOLLO_GATEWAY_URL, 30)
        .await
        .expect("Gateway should be ready");

    // Query SDL from gateway
    let response = graphql_query(
        APOLLO_GATEWAY_URL,
        "query { _service { sdl } }",
    )
    .await
    .expect("SDL query should succeed");

    // Should have data, no errors
    assert!(!has_errors(&response), "SDL query should not have errors");
    assert!(extract_data(&response).is_some(), "SDL query should return data");

    let sdl = extract_data(&response)
        .and_then(|d| d.get("_service"))
        .and_then(|s| s.get("sdl"))
        .and_then(|s| s.as_str())
        .expect("SDL should be a string");

    // Verify SDL contains federation directives
    assert!(
        sdl.contains("@key"),
        "SDL should contain @key directive"
    );
    assert!(
        sdl.contains("type User"),
        "SDL should contain User type"
    );
    assert!(
        sdl.contains("type Order"),
        "SDL should contain Order type"
    );
    assert!(
        sdl.contains("type Product"),
        "SDL should contain Product type"
    );
}

// ============================================================================
// Single Subgraph Query Tests
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_users_subgraph_query() {
    let response = graphql_query(
        USERS_SUBGRAPH_URL,
        "query { users { id identifier } }",
    )
    .await
    .expect("Query should succeed");

    assert!(!has_errors(&response), "Query should not have errors: {:?}", response.get("errors"));

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
#[ignore]
async fn test_orders_subgraph_query() {
    let response = graphql_query(
        ORDERS_SUBGRAPH_URL,
        "query { orders { id status total } }",
    )
    .await
    .expect("Query should succeed");

    assert!(!has_errors(&response), "Query should not have errors: {:?}", response.get("errors"));

    let data = extract_data(&response)
        .and_then(|d| d.get("orders"))
        .and_then(|o| o.as_array())
        .expect("Should return array of orders");

    assert!(!data.is_empty(), "Should have at least one order");
    println!("✓ Orders query succeeded with {} results", data.len());
}

#[tokio::test]
#[ignore]
async fn test_products_subgraph_query() {
    let response = graphql_query(
        PRODUCTS_SUBGRAPH_URL,
        "query { products { id name price } }",
    )
    .await
    .expect("Query should succeed");

    assert!(!has_errors(&response), "Query should not have errors: {:?}", response.get("errors"));

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
#[ignore]
async fn test_gateway_simple_query() {
    let response = graphql_query(
        APOLLO_GATEWAY_URL,
        "query { users { id identifier } }",
    )
    .await
    .expect("Query should succeed");

    assert!(!has_errors(&response), "Query should not have errors: {:?}", response.get("errors"));

    let data = extract_data(&response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .expect("Should return array of users");

    assert!(!data.is_empty(), "Should have at least one user");
    println!("✓ Gateway simple query succeeded");
}

#[tokio::test]
#[ignore]
async fn test_gateway_two_subgraph_federation() {
    // Query users with their orders (2-hop federation: gateway -> users -> orders)
    let query = r#"
        query {
            users {
                id
                identifier
                orders {
                    id
                    status
                    total
                }
            }
        }
    "#;

    let response = graphql_query(APOLLO_GATEWAY_URL, query)
        .await
        .expect("Query should succeed");

    if has_errors(&response) {
        eprintln!("GraphQL errors: {:?}", response.get("errors"));
    }

    assert!(!has_errors(&response), "Query should not have errors");

    let users = extract_data(&response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .expect("Should return users array");

    assert!(!users.is_empty(), "Should have at least one user");

    // At least some users should have orders
    let has_orders = users
        .iter()
        .any(|u| {
            u.get("orders")
                .and_then(|o| o.as_array())
                .map(|arr| !arr.is_empty())
                .unwrap_or(false)
        });

    assert!(has_orders, "Some users should have orders in federated query");
    println!("✓ Gateway 2-subgraph federation query succeeded");
}

#[tokio::test]
#[ignore]
async fn test_gateway_three_subgraph_federation() {
    // Query users with their orders and order products (3-hop federation)
    let query = r#"
        query {
            users {
                id
                identifier
                orders {
                    id
                    status
                    products {
                        id
                        name
                        price
                    }
                }
            }
        }
    "#;

    let response = graphql_query(APOLLO_GATEWAY_URL, query)
        .await
        .expect("Query should succeed");

    if has_errors(&response) {
        eprintln!("GraphQL errors: {:?}", response.get("errors"));
    }

    assert!(!has_errors(&response), "Query should not have errors");

    let users = extract_data(&response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .expect("Should return users array");

    assert!(!users.is_empty(), "Should have at least one user");
    println!("✓ Gateway 3-subgraph federation query succeeded");
}

// ============================================================================
// Entity Resolution Tests
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_user_entity_resolution() {
    // Query users to get IDs
    let users_response = graphql_query(
        USERS_SUBGRAPH_URL,
        "query { users(limit: 1) { id } }",
    )
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
    let query = format!(
        r#"query {{ user(id: "{}") {{ id identifier }} }}"#,
        user_id
    );

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
#[ignore]
async fn test_gateway_invalid_query_error_handling() {
    let response = graphql_query(
        APOLLO_GATEWAY_URL,
        "query { invalidField { subfield } }",
    )
    .await
    .expect("Request should complete even with invalid query");

    assert!(
        has_errors(&response),
        "Invalid query should return errors"
    );

    let errors = response
        .get("errors")
        .and_then(|e| e.as_array())
        .expect("Should have errors array");

    assert!(!errors.is_empty(), "Should have at least one error");
    println!("✓ Invalid query error handling works");
}

// ============================================================================
// Performance Tests
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_federation_query_performance() {
    let start = std::time::Instant::now();

    let _response = graphql_query(
        APOLLO_GATEWAY_URL,
        r#"query {
            users(limit: 5) {
                id
                orders {
                    id
                    products {
                        id
                    }
                }
            }
        }"#,
    )
    .await
    .expect("Query should succeed");

    let elapsed = start.elapsed();

    println!(
        "✓ 3-hop federated query completed in: {:.0}ms",
        elapsed.as_millis()
    );

    // Assert reasonable latency (adjust based on actual performance)
    assert!(
        elapsed.as_millis() < 1000,
        "Federated query should complete in under 1 second"
    );
}

// ============================================================================
// Test Helper Module
// ============================================================================

#[cfg(test)]
mod setup {
    use std::process::Command;

    /// Check if docker-compose is available
    #[allow(dead_code)]
    pub fn check_docker_compose() -> bool {
        Command::new("docker-compose")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Get current docker-compose status
    #[allow(dead_code)]
    pub fn get_status() {
        let output = Command::new("docker-compose")
            .arg("ps")
            .current_dir("tests/integration")
            .output();

        match output {
            Ok(output) => {
                println!("Docker Compose Status:");
                println!("{}", String::from_utf8_lossy(&output.stdout));
            }
            Err(e) => eprintln!("Failed to get docker-compose status: {}", e),
        }
    }
}

// ============================================================================
// Integration Test Suite
// ============================================================================

#[test]
fn test_suite_documentation() {
    println!("\n============================================================================");
    println!("Docker Compose Federation Integration Tests");
    println!("============================================================================");
    println!("\nTo run these tests:");
    println!("1. Start Docker Compose:");
    println!("   cd tests/integration && docker-compose up -d");
    println!("\n2. Run integration tests:");
    println!("   cargo test --test federation_docker_compose_integration -- --ignored --nocapture");
    println!("\n3. View logs:");
    println!("   docker-compose logs -f [service-name]");
    println!("\n4. Stop services:");
    println!("   docker-compose down -v");
    println!("============================================================================\n");
}
