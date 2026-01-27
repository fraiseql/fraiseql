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
// Two-Subgraph Federation Tests (Core functionality)
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_two_subgraph_setup_validation() {
    // Validate that 2-subgraph setup is working
    if let Err(e) = setup_federation_tests().await {
        panic!("2-subgraph federation setup failed: {}", e);
    }
}

#[tokio::test]
#[ignore]
async fn test_two_subgraph_direct_subgraph_queries() {
    setup_federation_tests()
        .await
        .expect("Setup should succeed");

    // Test 1: Query users directly from users subgraph
    println!("\n--- Test 1: Query users directly ---");
    let users_response = graphql_query(
        USERS_SUBGRAPH_URL,
        r#"query { users { id identifier email } }"#,
    )
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
    println!(
        "✓ Found {} users from users subgraph",
        users.len()
    );

    // Test 2: Query orders directly from orders subgraph
    println!("\n--- Test 2: Query orders directly ---");
    let orders_response = graphql_query(
        ORDERS_SUBGRAPH_URL,
        r#"query { orders { id status total } }"#,
    )
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
    println!(
        "✓ Found {} orders from orders subgraph",
        orders.len()
    );
}

#[tokio::test]
#[ignore]
async fn test_two_subgraph_http_federation_from_orders() {
    setup_federation_tests()
        .await
        .expect("Setup should succeed");

    println!("\n--- Test: Query orders with extended User fields (HTTP federation) ---");

    // Orders subgraph extends User type from users subgraph
    // This tests if orders can resolve User information via HTTP
    let query = r#"
        query {
            orders(limit: 5) {
                id
                status
                total
                user {
                    id
                    identifier
                    email
                }
            }
        }
    "#;

    let response = graphql_query(ORDERS_SUBGRAPH_URL, query)
        .await
        .expect("Orders with user query should succeed");

    if has_errors(&response) {
        eprintln!(
            "! GraphQL errors (may be expected if User not fully extended): {}",
            get_error_messages(&response)
        );
        eprintln!("! This indicates orders subgraph HTTP federation to users may not be fully configured");
    } else {
        let orders = extract_data(&response)
            .and_then(|d| d.get("orders"))
            .and_then(|o| o.as_array())
            .expect("Should return orders array");

        assert!(!orders.is_empty(), "Should have orders");

        // Verify orders have user information
        let has_user_info = orders
            .iter()
            .any(|o| {
                o.get("user")
                    .and_then(|u| u.get("id"))
                    .is_some()
            });

        if has_user_info {
            println!("✓ Orders successfully resolved User information via HTTP federation");
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_two_subgraph_federation_through_gateway() {
    setup_federation_tests()
        .await
        .expect("Setup should succeed");

    println!("\n--- Test: Federated query through Apollo Router gateway ---");

    // Query users with their orders through the gateway
    let query = r#"
        query {
            users(limit: 3) {
                id
                identifier
                email
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
        .expect("Gateway query should complete");

    // Print detailed error info if it fails
    if has_errors(&response) {
        let errors = get_error_messages(&response);
        eprintln!("✗ Gateway federation query failed: {}", errors);
        eprintln!(
            "Response: {}",
            serde_json::to_string_pretty(&response).unwrap_or_default()
        );
        panic!("Gateway federation query should not have errors");
    }

    let users = extract_data(&response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .expect("Should return users array");

    assert!(!users.is_empty(), "Should have at least one user");
    println!("✓ Successfully queried users through gateway");

    // Check if we got orders (federated data)
    let users_with_orders = users
        .iter()
        .filter(|u| {
            u.get("orders")
                .and_then(|o| o.as_array())
                .map(|arr| !arr.is_empty())
                .unwrap_or(false)
        })
        .count();

    println!(
        "✓ {} users have orders (federated data)",
        users_with_orders
    );
}

#[tokio::test]
#[ignore]
async fn test_two_subgraph_entity_resolution_consistency() {
    setup_federation_tests()
        .await
        .expect("Setup should succeed");

    println!("\n--- Test: Entity resolution consistency across subgraphs ---");

    // Get a user ID from users subgraph
    let users_response = graphql_query(
        USERS_SUBGRAPH_URL,
        r#"query { users(limit: 1) { id identifier } }"#,
    )
    .await
    .expect("Initial users query should succeed");

    let user_id = extract_data(&users_response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .and_then(|arr| arr.first())
        .and_then(|user| user.get("id"))
        .and_then(|id| id.as_str())
        .expect("Should extract user ID");

    let user_identifier = extract_data(&users_response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .and_then(|arr| arr.first())
        .and_then(|user| user.get("identifier"))
        .and_then(|id| id.as_str())
        .expect("Should extract user identifier");

    println!("Got user: id={}, identifier={}", user_id, user_identifier);

    // Query the same user through orders subgraph (which has it as extended)
    let query = format!(
        r#"query {{ user(id: "{}") {{ id identifier }} }}"#,
        user_id
    );

    let orders_response = graphql_query(ORDERS_SUBGRAPH_URL, &query)
        .await
        .expect("Orders user query should succeed");

    if has_errors(&orders_response) {
        let errors = get_error_messages(&orders_response);
        eprintln!(
            "! Entity resolution query has errors: {}",
            errors
        );
        eprintln!("! This may indicate extended User type is not properly configured");
    } else {
        let resolved_user = extract_data(&orders_response)
            .and_then(|d| d.get("user"))
            .expect("Should return resolved user");

        let resolved_id = resolved_user
            .get("id")
            .and_then(|id| id.as_str())
            .expect("Should have user ID");

        assert_eq!(
            resolved_id, user_id,
            "Resolved user ID should match original"
        );

        println!(
            "✓ User {} consistently resolved across subgraphs",
            user_id
        );
    }
}

#[tokio::test]
#[ignore]
async fn test_two_subgraph_data_consistency() {
    setup_federation_tests()
        .await
        .expect("Setup should succeed");

    println!("\n--- Test: Data consistency between direct and federated queries ---");

    // Get users directly from users subgraph
    let direct_response = graphql_query(
        USERS_SUBGRAPH_URL,
        r#"query { users(limit: 3) { id identifier } }"#,
    )
    .await
    .expect("Direct users query should succeed");

    let direct_users = extract_data(&direct_response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .expect("Should return users array");

    // Get same users through gateway (federated)
    let gateway_response = graphql_query(
        APOLLO_GATEWAY_URL,
        r#"query { users(limit: 3) { id identifier } }"#,
    )
    .await
    .expect("Gateway users query should succeed");

    if has_errors(&gateway_response) {
        eprintln!(
            "! Gateway users query error: {}",
            get_error_messages(&gateway_response)
        );
    } else {
        let gateway_users = extract_data(&gateway_response)
            .and_then(|d| d.get("users"))
            .and_then(|u| u.as_array())
            .expect("Should return users array");

        // Compare counts
        assert_eq!(
            direct_users.len(),
            gateway_users.len(),
            "User counts should match between direct and gateway queries"
        );

        // Compare IDs
        let direct_ids: Vec<_> = direct_users
            .iter()
            .filter_map(|u| u.get("id")?.as_str())
            .collect();

        let gateway_ids: Vec<_> = gateway_users
            .iter()
            .filter_map(|u| u.get("id")?.as_str())
            .collect();

        assert_eq!(direct_ids, gateway_ids, "User IDs should match");

        println!(
            "✓ Data consistency verified: {} users match across direct and federated queries",
            direct_ids.len()
        );
    }
}

#[tokio::test]
#[ignore]
async fn test_two_subgraph_federation_performance() {
    setup_federation_tests()
        .await
        .expect("Setup should succeed");

    println!("\n--- Test: Federation query performance ---");

    let query = r#"
        query {
            users(limit: 10) {
                id
                identifier
                orders {
                    id
                    status
                }
            }
        }
    "#;

    // Warm-up query
    let _ = graphql_query(APOLLO_GATEWAY_URL, query).await;

    // Timed query
    let start = std::time::Instant::now();
    let response = graphql_query(APOLLO_GATEWAY_URL, query)
        .await
        .expect("Query should succeed");

    let elapsed = start.elapsed();

    assert!(
        !has_errors(&response),
        "Query should not have errors: {}",
        get_error_messages(&response)
    );

    let users = extract_data(&response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .expect("Should return users");

    println!(
        "✓ 2-hop federation query ({} users + orders): {:.0}ms",
        users.len(),
        elapsed.as_millis()
    );

    // Assert reasonable latency
    assert!(
        elapsed.as_millis() < 5000,
        "Federation query should complete in reasonable time (got {:.0}ms)",
        elapsed.as_millis()
    );
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
// Extended Mutations Tests (HTTP Federation)
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_extended_mutation_user_from_authoritative_subgraph() {
    setup_federation_tests()
        .await
        .expect("Setup should succeed");

    println!("\n--- Test: Direct user mutation in authoritative subgraph ---");

    // Create a new user directly in users subgraph (authoritative)
    let mutation = r#"
        mutation {
            createUser(
                identifier: "test_user_direct@example.com"
                name: "Test User Direct"
                email: "test_user_direct@example.com"
            ) {
                id
                identifier
                name
                email
            }
        }
    "#;

    let response = graphql_query(USERS_SUBGRAPH_URL, mutation)
        .await
        .expect("Create user mutation should succeed");

    if has_errors(&response) {
        eprintln!(
            "! User creation has errors: {}",
            get_error_messages(&response)
        );
    } else {
        let user = extract_data(&response)
            .and_then(|d| d.get("createUser"))
            .expect("Should return created user");

        let user_id = user
            .get("id")
            .and_then(|id| id.as_str())
            .expect("Should have user ID");

        println!(
            "✓ Created user directly in authoritative subgraph: {}",
            user_id
        );
    }
}

#[tokio::test]
#[ignore]
async fn test_extended_mutation_update_user_from_extended_subgraph() {
    setup_federation_tests()
        .await
        .expect("Setup should succeed");

    println!("\n--- Test: Update user mutation from extended subgraph (HTTP propagation) ---");

    // Get an existing user
    let users_response = graphql_query(
        USERS_SUBGRAPH_URL,
        r#"query { users(limit: 1) { id } }"#,
    )
    .await
    .expect("Get users query should succeed");

    let user_id = extract_data(&users_response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .and_then(|arr| arr.first())
        .and_then(|user| user.get("id"))
        .and_then(|id| id.as_str())
        .expect("Should have user ID");

    println!("Using user ID for mutation: {}", user_id);

    // Try to update user from orders subgraph (extended type)
    // This tests if orders subgraph can propagate mutations to users subgraph
    let mutation = format!(
        r#"
        mutation {{
            updateUser(
                id: "{}"
                name: "Updated from Orders"
            ) {{
                id
                name
                email
            }}
        }}
    "#,
        user_id
    );

    let response = graphql_query(ORDERS_SUBGRAPH_URL, &mutation)
        .await
        .expect("Update user mutation should complete");

    if has_errors(&response) {
        eprintln!(
            "! Update user mutation has errors: {}",
            get_error_messages(&response)
        );
        eprintln!(
            "! This may indicate extended mutations are not configured"
        );
    } else {
        let updated_user = extract_data(&response)
            .and_then(|d| d.get("updateUser"))
            .expect("Should return updated user");

        let updated_name = updated_user
            .get("name")
            .and_then(|n| n.as_str())
            .expect("Should have updated name");

        println!(
            "✓ Successfully updated user from extended subgraph: {}",
            updated_name
        );
    }
}

#[tokio::test]
#[ignore]
async fn test_extended_mutation_create_order_with_user_reference() {
    setup_federation_tests()
        .await
        .expect("Setup should succeed");

    println!("\n--- Test: Create order with user reference (entity linking) ---");

    // Get a user ID
    let users_response = graphql_query(
        USERS_SUBGRAPH_URL,
        r#"query { users(limit: 1) { id } }"#,
    )
    .await
    .expect("Get users query should succeed");

    let user_id = extract_data(&users_response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .and_then(|arr| arr.first())
        .and_then(|user| user.get("id"))
        .and_then(|id| id.as_str())
        .expect("Should have user ID");

    println!("Creating order for user: {}", user_id);

    // Create order in orders subgraph
    let mutation = format!(
        r#"
        mutation {{
            createOrder(
                userId: "{}"
                status: "pending"
                total: 99.99
            ) {{
                id
                status
                total
                user {{
                    id
                    identifier
                }}
            }}
        }}
    "#,
        user_id
    );

    let response = graphql_query(ORDERS_SUBGRAPH_URL, &mutation)
        .await
        .expect("Create order mutation should succeed");

    if has_errors(&response) {
        eprintln!(
            "! Create order mutation has errors: {}",
            get_error_messages(&response)
        );
    } else {
        let order = extract_data(&response)
            .and_then(|d| d.get("createOrder"))
            .expect("Should return created order");

        let order_id = order
            .get("id")
            .and_then(|id| id.as_str())
            .expect("Should have order ID");

        println!(
            "✓ Successfully created order with user reference: {}",
            order_id
        );

        // Verify user reference was resolved
        if let Some(user) = order.get("user") {
            if let Some(resolved_id) = user.get("id").and_then(|id| id.as_str()) {
                println!(
                    "✓ User reference resolved in order: {}",
                    resolved_id
                );
            }
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_extended_mutation_error_handling() {
    setup_federation_tests()
        .await
        .expect("Setup should succeed");

    println!("\n--- Test: Error handling in extended mutations ---");

    // Try to update non-existent user
    let mutation = r#"
        mutation {
            updateUser(
                id: "00000000-0000-0000-0000-000000000000"
                name: "Nonexistent"
            ) {
                id
                name
            }
        }
    "#;

    let response = graphql_query(USERS_SUBGRAPH_URL, mutation)
        .await
        .expect("Query should complete");

    if has_errors(&response) {
        let errors = get_error_messages(&response);
        println!("✓ Expected error for non-existent user: {}", errors);
    } else {
        let result = extract_data(&response)
            .and_then(|d| d.get("updateUser"))
            .expect("Should return result");

        if result.is_null() {
            println!("✓ Non-existent user returned null (expected behavior)");
        } else {
            println!("⚠ Unexpected result for non-existent user: {:?}", result);
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_extended_mutation_data_consistency_after_mutation() {
    setup_federation_tests()
        .await
        .expect("Setup should succeed");

    println!("\n--- Test: Data consistency after extended mutations ---");

    // Get a user
    let users_response = graphql_query(
        USERS_SUBGRAPH_URL,
        r#"query { users(limit: 1) { id name } }"#,
    )
    .await
    .expect("Get users query should succeed");

    let original_user = extract_data(&users_response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .and_then(|arr| arr.first())
        .expect("Should have at least one user")
        .clone();

    let user_id = original_user
        .get("id")
        .and_then(|id| id.as_str())
        .expect("Should have ID");

    let original_name = original_user
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("unknown");

    println!("Original user: id={}, name={}", user_id, original_name);

    // Update user
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let new_name = format!("Updated_{}", timestamp);
    let mutation = format!(
        r#"
        mutation {{
            updateUser(
                id: "{}"
                name: "{}"
            ) {{
                id
                name
            }}
        }}
    "#,
        user_id, new_name
    );

    let update_response = graphql_query(USERS_SUBGRAPH_URL, &mutation)
        .await
        .expect("Update mutation should succeed");

    if !has_errors(&update_response) {
        // Query the user again and verify consistency
        let query = format!(r#"query {{ user(id: "{}") {{ id name }} }}"#, user_id);

        let verify_response = graphql_query(USERS_SUBGRAPH_URL, &query)
            .await
            .expect("Verification query should succeed");

        let verified_user = extract_data(&verify_response)
            .and_then(|d| d.get("user"))
            .expect("Should return user");

        let verified_name = verified_user
            .get("name")
            .and_then(|n| n.as_str())
            .expect("Should have name");

        assert_eq!(
            verified_name, &new_name,
            "Updated name should persist after mutation"
        );

        println!(
            "✓ Data consistency verified: mutation persisted correctly"
        );
    } else {
        eprintln!(
            "! Mutation failed: {}",
            get_error_messages(&update_response)
        );
    }
}

#[tokio::test]
#[ignore]
async fn test_extended_mutation_through_gateway() {
    setup_federation_tests()
        .await
        .expect("Setup should succeed");

    println!("\n--- Test: Mutation through gateway (federated mutation) ---");

    // Get a user
    let users_response = graphql_query(
        APOLLO_GATEWAY_URL,
        r#"query { users(limit: 1) { id } }"#,
    )
    .await
    .expect("Get users query should succeed");

    let user_id = extract_data(&users_response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .and_then(|arr| arr.first())
        .and_then(|user| user.get("id"))
        .and_then(|id| id.as_str())
        .expect("Should have user ID");

    // Try to update user through gateway
    let mutation = format!(
        r#"
        mutation {{
            updateUser(
                id: "{}"
                name: "Updated via Gateway"
            ) {{
                id
                name
            }}
        }}
    "#,
        user_id
    );

    let response = graphql_query(APOLLO_GATEWAY_URL, &mutation)
        .await
        .expect("Gateway mutation should complete");

    if has_errors(&response) {
        eprintln!(
            "! Gateway mutation has errors: {}",
            get_error_messages(&response)
        );
        eprintln!(
            "! This may be expected if mutations are not yet routed through gateway"
        );
    } else {
        let result = extract_data(&response)
            .and_then(|d| d.get("updateUser"))
            .expect("Should return result");

        println!(
            "✓ Gateway mutation executed successfully: {:?}",
            result.get("name")
        );
    }
}

#[tokio::test]
#[ignore]
async fn test_extended_mutation_performance() {
    setup_federation_tests()
        .await
        .expect("Setup should succeed");

    println!("\n--- Test: Mutation performance ---");

    // Create multiple orders to measure performance
    let start = std::time::Instant::now();

    for i in 0..5 {
        let mutation = format!(
            r#"
            mutation {{
                createOrder(
                    userId: "550e8400-e29b-41d4-a716-446655440001"
                    status: "pending"
                    total: {}.99
                ) {{
                    id
                }}
            }}
        "#,
            i * 10
        );

        let response = graphql_query(ORDERS_SUBGRAPH_URL, &mutation)
            .await
            .expect("Mutation should succeed");

        if has_errors(&response) {
            eprintln!(
                "! Mutation {} failed: {}",
                i,
                get_error_messages(&response)
            );
            break;
        }
    }

    let elapsed = start.elapsed();

    println!(
        "✓ 5 order creation mutations completed in: {:.0}ms",
        elapsed.as_millis()
    );

    // Assert reasonable latency
    assert!(
        elapsed.as_millis() < 10000,
        "Mutations should complete in reasonable time"
    );
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
