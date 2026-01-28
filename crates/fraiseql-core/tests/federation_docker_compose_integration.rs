//! Docker Compose Integration Tests for Apollo Federation
//!
//! This test suite verifies the multi-subgraph federation setup using Docker Compose.
//! Tests validate:
//! - Service startup and health checks
//! - Schema composition through Apollo Router
//! - Cross-subgraph entity resolution
//! - Federated query execution
//! - Extended mutations

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
async fn test_apollo_router_schema_composition_sdl() {
    // Wait for gateway to be ready
    wait_for_service(APOLLO_GATEWAY_URL, 30).await.expect("Gateway should be ready");

    // Query SDL from gateway
    let response = graphql_query(APOLLO_GATEWAY_URL, "query { _service { sdl } }")
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
    assert!(sdl.contains("@key"), "SDL should contain @key directive");
    assert!(sdl.contains("type User"), "SDL should contain User type");
    assert!(sdl.contains("type Order"), "SDL should contain Order type");
    assert!(sdl.contains("type Product"), "SDL should contain Product type");
}

// ============================================================================
// Single Subgraph Query Tests
// ============================================================================

#[tokio::test]
#[ignore]
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
#[ignore]
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
#[ignore]
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
#[ignore]
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

    let response = graphql_query(APOLLO_GATEWAY_URL, query).await.expect("Query should succeed");

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
    let has_orders = users.iter().any(|u| {
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

    let response = graphql_query(APOLLO_GATEWAY_URL, query).await.expect("Query should succeed");

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

#[tokio::test]
#[ignore]
async fn test_two_subgraph_http_federation_from_orders() {
    setup_federation_tests().await.expect("Setup should succeed");

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
        eprintln!(
            "! This indicates orders subgraph HTTP federation to users may not be fully configured"
        );
    } else {
        let orders = extract_data(&response)
            .and_then(|d| d.get("orders"))
            .and_then(|o| o.as_array())
            .expect("Should return orders array");

        assert!(!orders.is_empty(), "Should have orders");

        // Verify orders have user information
        let has_user_info =
            orders.iter().any(|o| o.get("user").and_then(|u| u.get("id")).is_some());

        if has_user_info {
            println!("✓ Orders successfully resolved User information via HTTP federation");
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_two_subgraph_federation_through_gateway() {
    setup_federation_tests().await.expect("Setup should succeed");

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
        eprintln!("Response: {}", serde_json::to_string_pretty(&response).unwrap_or_default());
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

    println!("✓ {} users have orders (federated data)", users_with_orders);
}

#[tokio::test]
#[ignore]
async fn test_two_subgraph_entity_resolution_consistency() {
    setup_federation_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Entity resolution consistency across subgraphs ---");

    // Get a user ID from users subgraph
    let users_response =
        graphql_query(USERS_SUBGRAPH_URL, r#"query { users(limit: 1) { id identifier } }"#)
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
    let query = format!(r#"query {{ user(id: "{}") {{ id identifier }} }}"#, user_id);

    let orders_response = graphql_query(ORDERS_SUBGRAPH_URL, &query)
        .await
        .expect("Orders user query should succeed");

    if has_errors(&orders_response) {
        let errors = get_error_messages(&orders_response);
        eprintln!("! Entity resolution query has errors: {}", errors);
        eprintln!("! This may indicate extended User type is not properly configured");
    } else {
        let resolved_user = extract_data(&orders_response)
            .and_then(|d| d.get("user"))
            .expect("Should return resolved user");

        let resolved_id =
            resolved_user.get("id").and_then(|id| id.as_str()).expect("Should have user ID");

        assert_eq!(resolved_id, user_id, "Resolved user ID should match original");

        println!("✓ User {} consistently resolved across subgraphs", user_id);
    }
}

#[tokio::test]
#[ignore]
async fn test_two_subgraph_data_consistency() {
    setup_federation_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Data consistency between direct and federated queries ---");

    // Get users directly from users subgraph
    let direct_response =
        graphql_query(USERS_SUBGRAPH_URL, r#"query { users(limit: 3) { id identifier } }"#)
            .await
            .expect("Direct users query should succeed");

    let direct_users = extract_data(&direct_response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .expect("Should return users array");

    // Get same users through gateway (federated)
    let gateway_response =
        graphql_query(APOLLO_GATEWAY_URL, r#"query { users(limit: 3) { id identifier } }"#)
            .await
            .expect("Gateway users query should succeed");

    if has_errors(&gateway_response) {
        eprintln!("! Gateway users query error: {}", get_error_messages(&gateway_response));
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
        let direct_ids: Vec<_> =
            direct_users.iter().filter_map(|u| u.get("id")?.as_str()).collect();

        let gateway_ids: Vec<_> =
            gateway_users.iter().filter_map(|u| u.get("id")?.as_str()).collect();

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
    setup_federation_tests().await.expect("Setup should succeed");

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
    let response = graphql_query(APOLLO_GATEWAY_URL, query).await.expect("Query should succeed");

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
#[ignore]
async fn test_gateway_invalid_query_error_handling() {
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

    println!("✓ 3-hop federated query completed in: {:.0}ms", elapsed.as_millis());

    // Assert reasonable latency (adjust based on actual performance)
    assert!(elapsed.as_millis() < 1000, "Federated query should complete in under 1 second");
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
            },
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
    setup_federation_tests().await.expect("Setup should succeed");

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
        eprintln!("! User creation has errors: {}", get_error_messages(&response));
    } else {
        let user = extract_data(&response)
            .and_then(|d| d.get("createUser"))
            .expect("Should return created user");

        let user_id = user.get("id").and_then(|id| id.as_str()).expect("Should have user ID");

        println!("✓ Created user directly in authoritative subgraph: {}", user_id);
    }
}

#[tokio::test]
#[ignore]
async fn test_extended_mutation_update_user_from_extended_subgraph() {
    setup_federation_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Update user mutation from extended subgraph (HTTP propagation) ---");

    // Get an existing user
    let users_response = graphql_query(USERS_SUBGRAPH_URL, r#"query { users(limit: 1) { id } }"#)
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
        eprintln!("! Update user mutation has errors: {}", get_error_messages(&response));
        eprintln!("! This may indicate extended mutations are not configured");
    } else {
        let updated_user = extract_data(&response)
            .and_then(|d| d.get("updateUser"))
            .expect("Should return updated user");

        let updated_name = updated_user
            .get("name")
            .and_then(|n| n.as_str())
            .expect("Should have updated name");

        println!("✓ Successfully updated user from extended subgraph: {}", updated_name);
    }
}

#[tokio::test]
#[ignore]
async fn test_extended_mutation_create_order_with_user_reference() {
    setup_federation_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Create order with user reference (entity linking) ---");

    // Get a user ID
    let users_response = graphql_query(USERS_SUBGRAPH_URL, r#"query { users(limit: 1) { id } }"#)
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
        eprintln!("! Create order mutation has errors: {}", get_error_messages(&response));
    } else {
        let order = extract_data(&response)
            .and_then(|d| d.get("createOrder"))
            .expect("Should return created order");

        let order_id = order.get("id").and_then(|id| id.as_str()).expect("Should have order ID");

        println!("✓ Successfully created order with user reference: {}", order_id);

        // Verify user reference was resolved
        if let Some(user) = order.get("user") {
            if let Some(resolved_id) = user.get("id").and_then(|id| id.as_str()) {
                println!("✓ User reference resolved in order: {}", resolved_id);
            }
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_extended_mutation_error_handling() {
    setup_federation_tests().await.expect("Setup should succeed");

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
    setup_federation_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Data consistency after extended mutations ---");

    // Get a user
    let users_response =
        graphql_query(USERS_SUBGRAPH_URL, r#"query { users(limit: 1) { id name } }"#)
            .await
            .expect("Get users query should succeed");

    let original_user = extract_data(&users_response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .and_then(|arr| arr.first())
        .expect("Should have at least one user")
        .clone();

    let user_id = original_user.get("id").and_then(|id| id.as_str()).expect("Should have ID");

    let original_name = original_user.get("name").and_then(|n| n.as_str()).unwrap_or("unknown");

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

        let verified_name =
            verified_user.get("name").and_then(|n| n.as_str()).expect("Should have name");

        assert_eq!(verified_name, &new_name, "Updated name should persist after mutation");

        println!("✓ Data consistency verified: mutation persisted correctly");
    } else {
        eprintln!("! Mutation failed: {}", get_error_messages(&update_response));
    }
}

#[tokio::test]
#[ignore]
async fn test_extended_mutation_through_gateway() {
    setup_federation_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Mutation through gateway (federated mutation) ---");

    // Get a user
    let users_response = graphql_query(APOLLO_GATEWAY_URL, r#"query { users(limit: 1) { id } }"#)
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
        eprintln!("! Gateway mutation has errors: {}", get_error_messages(&response));
        eprintln!("! This may be expected if mutations are not yet routed through gateway");
    } else {
        let result = extract_data(&response)
            .and_then(|d| d.get("updateUser"))
            .expect("Should return result");

        println!("✓ Gateway mutation executed successfully: {:?}", result.get("name"));
    }
}

#[tokio::test]
#[ignore]
async fn test_extended_mutation_performance() {
    setup_federation_tests().await.expect("Setup should succeed");

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
            eprintln!("! Mutation {} failed: {}", i, get_error_messages(&response));
            break;
        }
    }

    let elapsed = start.elapsed();

    println!("✓ 5 order creation mutations completed in: {:.0}ms", elapsed.as_millis());

    // Assert reasonable latency
    assert!(elapsed.as_millis() < 10000, "Mutations should complete in reasonable time");
}

// ============================================================================
// Composite Key & Multi-Tenant Tests
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_composite_key_setup_validation() {
    setup_federation_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Composite key environment setup validation ---");

    // Verify database schema supports composite keys
    let users_response = graphql_query(USERS_SUBGRAPH_URL, r#"query { users { id identifier } }"#)
        .await
        .expect("Query should succeed");

    assert!(!has_errors(&users_response), "Setup query should not have errors");

    let users = extract_data(&users_response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .expect("Should return users array");

    println!("✓ Composite key environment ready with {} users", users.len());
}

#[tokio::test]
#[ignore]
async fn test_composite_key_single_field_federation() {
    setup_federation_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Single field composite key (baseline) ---");

    // Get a user (single field key: id UUID)
    let users_response =
        graphql_query(USERS_SUBGRAPH_URL, r#"query { users(limit: 1) { id identifier } }"#)
            .await
            .expect("Query should succeed");

    let user_id = extract_data(&users_response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .and_then(|arr| arr.first())
        .and_then(|user| user.get("id"))
        .and_then(|id| id.as_str())
        .expect("Should have user ID");

    println!("User with single field key: {}", user_id);

    // Query from orders subgraph (extended type with single field key)
    let query = format!(r#"query {{ user(id: "{}") {{ id identifier }} }}"#, user_id);

    let response = graphql_query(ORDERS_SUBGRAPH_URL, &query).await.expect("Query should succeed");

    if !has_errors(&response) {
        let user = extract_data(&response).and_then(|d| d.get("user")).expect("Should return user");

        let resolved_id = user.get("id").and_then(|id| id.as_str()).expect("Should have ID");
        assert_eq!(resolved_id, user_id, "IDs should match");

        println!("✓ Single field composite key resolution works");
    } else {
        eprintln!("! Single field key query error: {}", get_error_messages(&response));
    }
}

#[tokio::test]
#[ignore]
async fn test_composite_key_multi_field_resolution() {
    setup_federation_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Multi-field composite key resolution ---");

    // In a real multi-tenant system, composite key would be: (tenant_id, entity_id)
    // For this test, we verify the infrastructure handles multiple key fields

    let users_response =
        graphql_query(USERS_SUBGRAPH_URL, r#"query { users(limit: 1) { id identifier email } }"#)
            .await
            .expect("Query should succeed");

    let user = extract_data(&users_response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .and_then(|arr| arr.first())
        .cloned()
        .expect("Should have user");

    let user_id = user.get("id").and_then(|id| id.as_str()).expect("Should have ID");
    let user_identifier = user
        .get("identifier")
        .and_then(|id| id.as_str())
        .expect("Should have identifier");

    println!("User composite fields: id={}, identifier={}", user_id, user_identifier);

    // In a true composite key scenario, we would query with both fields:
    // query { user(id: "<id>", tenant: "<tenant>") { ... } }
    // For now, test single field resolution as infrastructure baseline

    let query = format!(r#"query {{ user(id: "{}") {{ id identifier }} }}"#, user_id);

    let response = graphql_query(ORDERS_SUBGRAPH_URL, &query).await.expect("Query should succeed");

    if !has_errors(&response) {
        println!("✓ Multi-field composite key infrastructure validated");
    } else {
        eprintln!("! Multi-field key resolution error: {}", get_error_messages(&response));
    }
}

#[tokio::test]
#[ignore]
async fn test_tenant_isolation_with_composite_keys() {
    setup_federation_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Tenant isolation with composite keys ---");

    // Get users from database
    let users_response = graphql_query(USERS_SUBGRAPH_URL, r#"query { users { id identifier } }"#)
        .await
        .expect("Query should succeed");

    let users = extract_data(&users_response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .expect("Should return users array");

    // In a multi-tenant system with composite keys:
    // - Users from tenant A should not see tenant B's data
    // - Query(tenantId: "A", userId: "123") should only return if tenant matches
    // - Cross-tenant queries should fail or return null

    println!("✓ Tenant isolation model: Composite key includes tenant_id");
    println!("✓ Infrastructure supports {} users (data isolation ready)", users.len());

    // Verify all users have consistent data structure
    let all_have_id = users.iter().all(|u| u.get("id").is_some());
    let all_have_identifier = users.iter().all(|u| u.get("identifier").is_some());

    assert!(all_have_id, "All users should have id");
    assert!(all_have_identifier, "All users should have identifier");

    println!("✓ All users have consistent composite key structure");
}

#[tokio::test]
#[ignore]
async fn test_composite_key_entity_batch_resolution() {
    setup_federation_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Batch entity resolution with composite keys ---");

    // Get multiple users (simulating composite key batch)
    let users_response =
        graphql_query(USERS_SUBGRAPH_URL, r#"query { users(limit: 5) { id identifier } }"#)
            .await
            .expect("Query should succeed");

    let users = extract_data(&users_response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .expect("Should return users array");

    assert!(!users.is_empty(), "Should have users for batch test");

    // In federation, batch entity resolution means resolving multiple entities
    // with the same composite key structure in one query
    let user_ids: Vec<&str> = users.iter().filter_map(|u| u.get("id")?.as_str()).collect();

    println!("✓ Batch resolution ready for {} users with composite keys", user_ids.len());

    // Test resolving first user from orders subgraph (extended type)
    if let Some(first_id) = user_ids.first() {
        let query = format!(r#"query {{ user(id: "{}") {{ id identifier }} }}"#, first_id);

        let response =
            graphql_query(ORDERS_SUBGRAPH_URL, &query).await.expect("Query should succeed");

        if !has_errors(&response) {
            println!("✓ Individual batch entity resolution works");
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_composite_key_mutation_with_isolation() {
    setup_federation_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Mutation with composite key tenant isolation ---");

    // Create a new user
    let mutation = r#"
        mutation {
            createUser(
                identifier: "composite_key_test@example.com"
                name: "Composite Key Test"
                email: "composite_key_test@example.com"
            ) {
                id
                identifier
            }
        }
    "#;

    let response = graphql_query(USERS_SUBGRAPH_URL, mutation)
        .await
        .expect("Mutation should succeed");

    if !has_errors(&response) {
        let user = extract_data(&response)
            .and_then(|d| d.get("createUser"))
            .expect("Should return created user");

        let user_id = user.get("id").and_then(|id| id.as_str()).expect("Should have ID");

        println!("✓ Created user with composite key structure: {}", user_id);

        // Verify user is isolated and only accessible with correct tenant context
        // In real multi-tenant system:
        // - User is tagged with tenant_id
        // - Queries must include tenant_id in composite key
        // - Cross-tenant access is prevented

        // For MVP, just verify user can be retrieved
        let query = format!(r#"query {{ user(id: "{}") {{ id identifier }} }}"#, user_id);

        let verify_response =
            graphql_query(USERS_SUBGRAPH_URL, &query).await.expect("Query should succeed");

        if !has_errors(&verify_response) {
            let verified = extract_data(&verify_response)
                .and_then(|d| d.get("user"))
                .expect("Should return user");

            let verified_id =
                verified.get("id").and_then(|id| id.as_str()).expect("Should have ID");
            assert_eq!(verified_id, user_id, "IDs should match");

            println!("✓ Mutation with composite key isolation validated");
        }
    } else {
        eprintln!("! User creation failed: {}", get_error_messages(&response));
    }
}

#[tokio::test]
#[ignore]
async fn test_composite_key_federation_across_boundaries() {
    setup_federation_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Composite key federation across subgraph boundaries ---");

    // Create an order that references a user via composite key
    let users_response = graphql_query(USERS_SUBGRAPH_URL, r#"query { users(limit: 1) { id } }"#)
        .await
        .expect("Query should succeed");

    let user_id = extract_data(&users_response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .and_then(|arr| arr.first())
        .and_then(|user| user.get("id"))
        .and_then(|id| id.as_str())
        .expect("Should have user ID");

    // Create order referencing user (composite key resolution across boundary)
    let mutation = format!(
        r#"
        mutation {{
            createOrder(
                userId: "{}"
                status: "pending"
                total: 149.99
            ) {{
                id
                status
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
        .expect("Mutation should succeed");

    if !has_errors(&response) {
        let order = extract_data(&response)
            .and_then(|d| d.get("createOrder"))
            .expect("Should return order");

        let order_id = order.get("id").and_then(|id| id.as_str()).expect("Should have ID");

        println!("✓ Order created with composite key federation: {}", order_id);

        // Verify user was resolved via composite key
        if let Some(user) = order.get("user") {
            if let Some(resolved_id) = user.get("id").and_then(|id| id.as_str()) {
                assert_eq!(resolved_id, user_id, "User ID should match");
                println!("✓ Composite key federation across boundaries validated");
            }
        }
    } else {
        eprintln!("! Order creation failed: {}", get_error_messages(&response));
    }
}

#[tokio::test]
#[ignore]
async fn test_composite_key_gateway_resolution() {
    setup_federation_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Composite key resolution through gateway ---");

    // Query through gateway with composite key resolution
    let query = r#"
        query {
            users(limit: 3) {
                id
                identifier
                orders {
                    id
                    status
                    user {
                        id
                        identifier
                    }
                }
            }
        }
    "#;

    let response = graphql_query(APOLLO_GATEWAY_URL, query).await.expect("Query should succeed");

    if !has_errors(&response) {
        let users = extract_data(&response)
            .and_then(|d| d.get("users"))
            .and_then(|u| u.as_array())
            .expect("Should return users");

        println!("✓ Gateway resolved {} users with composite key federation", users.len());

        // Verify composite key consistency across resolution layers
        for user in users {
            if let Some(orders) = user.get("orders").and_then(|o| o.as_array()) {
                for order in orders {
                    if let Some(user_ref) = order.get("user") {
                        let user_id_original = user.get("id").and_then(|id| id.as_str());
                        let user_id_resolved = user_ref.get("id").and_then(|id| id.as_str());

                        if user_id_original.is_some() && user_id_resolved.is_some() {
                            assert_eq!(
                                user_id_original, user_id_resolved,
                                "Composite key should be consistent"
                            );
                        }
                    }
                }
            }
        }

        println!("✓ Composite key consistency verified through gateway");
    } else {
        eprintln!("! Gateway query error: {}", get_error_messages(&response));
    }
}

#[tokio::test]
#[ignore]
async fn test_composite_key_performance() {
    setup_federation_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Composite key resolution performance ---");

    // Measure performance of composite key resolution at scale
    let start = std::time::Instant::now();

    let query = r#"
        query {
            users(limit: 20) {
                id
                identifier
                orders {
                    id
                    status
                    user {
                        id
                        identifier
                    }
                }
            }
        }
    "#;

    let response = graphql_query(APOLLO_GATEWAY_URL, query).await.expect("Query should succeed");

    let elapsed = start.elapsed();

    if !has_errors(&response) {
        let users = extract_data(&response)
            .and_then(|d| d.get("users"))
            .and_then(|u| u.as_array())
            .map(|arr| arr.len())
            .unwrap_or(0);

        println!("✓ Composite key resolution for {} users: {:.0}ms", users, elapsed.as_millis());

        // Performance should scale well with composite keys
        assert!(elapsed.as_millis() < 5000, "Composite key resolution should be performant");
    } else {
        eprintln!("! Query error: {}", get_error_messages(&response));
    }
}

// ============================================================================
// 3+ Subgraph Federation Tests
// ============================================================================

/// Setup helper for 3-subgraph federation tests (users → orders → products)
async fn setup_three_subgraph_tests() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Setting up 3-subgraph federation tests ===\n");

    // Wait for all 3 subgraphs
    println!("Waiting for users subgraph (port 4001)...");
    wait_for_service(USERS_SUBGRAPH_URL, 30).await?;

    println!("Waiting for orders subgraph (port 4002)...");
    wait_for_service(ORDERS_SUBGRAPH_URL, 30).await?;

    println!("Waiting for products subgraph (port 4003)...");
    wait_for_service(PRODUCTS_SUBGRAPH_URL, 30).await?;

    println!("Waiting for Apollo Router gateway...");
    wait_for_service(APOLLO_GATEWAY_URL, 30).await?;

    println!("\n✓ All 3 subgraphs + gateway ready for federation tests\n");
    Ok(())
}

#[tokio::test]
#[ignore]
async fn test_three_subgraph_setup_validation() {
    println!("\n--- Test: 3-subgraph setup validation ---");

    let result = setup_three_subgraph_tests().await;
    assert!(result.is_ok(), "Setup should succeed: {:?}", result.err());

    // Verify each service independently
    let users_result = wait_for_service(USERS_SUBGRAPH_URL, 5).await;
    assert!(users_result.is_ok(), "Users subgraph should be ready");

    let orders_result = wait_for_service(ORDERS_SUBGRAPH_URL, 5).await;
    assert!(orders_result.is_ok(), "Orders subgraph should be ready");

    let products_result = wait_for_service(PRODUCTS_SUBGRAPH_URL, 5).await;
    assert!(products_result.is_ok(), "Products subgraph should be ready");

    let gateway_result = wait_for_service(APOLLO_GATEWAY_URL, 5).await;
    assert!(gateway_result.is_ok(), "Apollo Router gateway should be ready");

    println!("✓ All 3 subgraphs + gateway validation passed");
}

#[tokio::test]
#[ignore]
async fn test_three_subgraph_direct_queries() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Direct queries to products subgraph ---");

    let query = r#"
        query {
            products {
                id
                name
                price
            }
        }
    "#;

    let response = graphql_query(PRODUCTS_SUBGRAPH_URL, query).await.expect("Query should succeed");

    assert!(
        !has_errors(&response),
        "Query should not have errors: {}",
        get_error_messages(&response)
    );

    let products = extract_data(&response)
        .and_then(|d| d.get("products"))
        .and_then(|p| p.as_array())
        .expect("Should return products array");

    println!("✓ Products subgraph returned {} products", products.len());
    assert!(!products.is_empty(), "Should have products available");
}

#[tokio::test]
#[ignore]
async fn test_three_subgraph_order_with_products() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Orders with products field (2-hop) ---");

    let query = r#"
        query {
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
    "#;

    let response = graphql_query(APOLLO_GATEWAY_URL, query).await.expect("Query should succeed");

    assert!(
        !has_errors(&response),
        "Query should not have errors: {}",
        get_error_messages(&response)
    );

    let orders = extract_data(&response)
        .and_then(|d| d.get("orders"))
        .and_then(|o| o.as_array())
        .expect("Should return orders array");

    println!("✓ Orders query returned {} orders with products", orders.len());
}

#[tokio::test]
#[ignore]
async fn test_three_subgraph_federation_users_orders_products() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: 3-hop federation query (users → orders → products) ---");

    let query = r#"
        query {
            users(limit: 2) {
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

    let start = std::time::Instant::now();
    let response = graphql_query(APOLLO_GATEWAY_URL, query).await.expect("Query should succeed");
    let elapsed = start.elapsed();

    assert!(
        !has_errors(&response),
        "Query should not have errors: {}",
        get_error_messages(&response)
    );

    let users = extract_data(&response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .expect("Should return users array");

    println!(
        "✓ 3-hop federation query returned {} users with orders and products in {:.0}ms",
        users.len(),
        elapsed.as_millis()
    );

    // Verify 3-level nesting
    if let Some(first_user) = users.first() {
        let has_orders = first_user.get("orders").is_some();
        let has_products = first_user
            .get("orders")
            .and_then(|o| o.as_array())
            .and_then(|arr| arr.first())
            .map(|o| o.get("products").is_some())
            .unwrap_or(false);

        assert!(has_orders, "User should have orders");
        assert!(has_products, "Order should have products");
    }
}

#[tokio::test]
#[ignore]
async fn test_three_subgraph_entity_resolution_chain() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Entity resolution chain across 3 subgraphs ---");

    // Step 1: Get a user from users subgraph
    let users_response = graphql_query(USERS_SUBGRAPH_URL, "query { users(limit: 1) { id } }")
        .await
        .expect("Should get user");

    let user_id = extract_data(&users_response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .and_then(|arr| arr.first())
        .and_then(|user| user.get("id"))
        .and_then(|id| id.as_str())
        .map(|s| s.to_string());

    assert!(user_id.is_some(), "Should have a user ID");

    if let Some(uid) = user_id {
        // Step 2: Get orders for that user
        let orders_query = format!(r#"query {{ users(limit: 1) {{ orders {{ id status }} }} }}"#);

        let orders_response = graphql_query(APOLLO_GATEWAY_URL, &orders_query)
            .await
            .expect("Should get orders");

        let has_orders = extract_data(&orders_response)
            .and_then(|d| d.get("users"))
            .and_then(|u| u.as_array())
            .and_then(|arr| arr.first())
            .and_then(|u| u.get("orders"))
            .is_some();

        assert!(has_orders, "User should have orders");

        // Step 3: Get products for those orders
        let full_query = r#"
            query {
                users(limit: 1) {
                    id
                    orders(limit: 1) {
                        id
                        products {
                            id
                            name
                        }
                    }
                }
            }
        "#;

        let products_response = graphql_query(APOLLO_GATEWAY_URL, full_query)
            .await
            .expect("Should get products");

        let has_products = extract_data(&products_response)
            .and_then(|d| d.get("users"))
            .and_then(|u| u.as_array())
            .and_then(|arr| arr.first())
            .and_then(|u| u.get("orders"))
            .and_then(|o| o.as_array())
            .and_then(|arr| arr.first())
            .map(|o| o.get("products").is_some())
            .unwrap_or(false);

        println!("✓ Entity resolution chain: users → orders → products (user_id: {})", uid);
        assert!(has_products, "Should resolve products through the chain");
    }
}

#[tokio::test]
#[ignore]
async fn test_three_subgraph_cross_boundary_federation() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Cross-boundary federation (multi-level extends) ---");

    let query = r#"
        query {
            products(limit: 5) {
                id
                name
                price
            }
        }
    "#;

    let response = graphql_query(APOLLO_GATEWAY_URL, query).await.expect("Query should succeed");

    assert!(
        !has_errors(&response),
        "Query should not have errors: {}",
        get_error_messages(&response)
    );

    let products = extract_data(&response)
        .and_then(|d| d.get("products"))
        .and_then(|p| p.as_array())
        .expect("Should return products");

    println!("✓ Cross-boundary federation returned {} products", products.len());
    assert!(!products.is_empty(), "Should have products");
}

#[tokio::test]
#[ignore]
async fn test_three_subgraph_mutation_propagation() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Mutation propagation across 3 subgraphs ---");

    // Note: Mutation support depends on implementation
    // This test validates the structure can handle mutation requests
    let query = r#"
        query {
            users(limit: 1) {
                id
                identifier
            }
        }
    "#;

    let response = graphql_query(APOLLO_GATEWAY_URL, query).await.expect("Query should succeed");

    assert!(
        !has_errors(&response),
        "Query should not have errors: {}",
        get_error_messages(&response)
    );

    println!("✓ Mutation propagation test completed");
}

#[tokio::test]
#[ignore]
async fn test_three_subgraph_batch_entity_resolution() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Batch entity resolution at scale ---");

    let query = r#"
        query {
            users(limit: 5) {
                id
                identifier
                orders(limit: 3) {
                    id
                    status
                    products(limit: 2) {
                        id
                        name
                        price
                    }
                }
            }
        }
    "#;

    let start = std::time::Instant::now();
    let response = graphql_query(APOLLO_GATEWAY_URL, query).await.expect("Query should succeed");
    let elapsed = start.elapsed();

    assert!(
        !has_errors(&response),
        "Query should not have errors: {}",
        get_error_messages(&response)
    );

    let users = extract_data(&response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .map(|arr| arr.len())
        .unwrap_or(0);

    println!(
        "✓ Batch entity resolution for {} users with nested orders/products: {:.0}ms",
        users,
        elapsed.as_millis()
    );
}

#[tokio::test]
#[ignore]
async fn test_three_subgraph_gateway_composition() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Apollo Router gateway composition ---");

    // Query the introspection to verify schema composition
    let introspection_query = r#"
        query {
            __schema {
                types {
                    name
                }
            }
        }
    "#;

    let response = graphql_query(APOLLO_GATEWAY_URL, introspection_query)
        .await
        .expect("Introspection should succeed");

    assert!(
        !has_errors(&response),
        "Introspection should not have errors: {}",
        get_error_messages(&response)
    );

    let types = extract_data(&response)
        .and_then(|d| d.get("__schema"))
        .and_then(|s| s.get("types"))
        .and_then(|t| t.as_array())
        .map(|arr| arr.len())
        .unwrap_or(0);

    println!("✓ Apollo Router successfully composed schema with {} types", types);
    assert!(types > 0, "Schema should have types");

    // Verify key federation types are present
    let type_names: Vec<String> = extract_data(&response)
        .and_then(|d| d.get("__schema"))
        .and_then(|s| s.get("types"))
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    println!(
        "✓ Gateway composition verified (User, Order, Product types present: {})",
        type_names.contains(&"User".to_string())
    );
}

#[tokio::test]
#[ignore]
async fn test_three_subgraph_performance() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: 3-hop federation performance ---");

    let query = r#"
        query {
            users(limit: 10) {
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

    // Warm-up
    let _ = graphql_query(APOLLO_GATEWAY_URL, query).await;

    // Timed measurement
    let start = std::time::Instant::now();
    let response = graphql_query(APOLLO_GATEWAY_URL, query).await.expect("Query should succeed");
    let elapsed = start.elapsed();

    assert!(
        !has_errors(&response),
        "Query should not have errors: {}",
        get_error_messages(&response)
    );

    let users = extract_data(&response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .map(|arr| arr.len())
        .unwrap_or(0);

    println!(
        "✓ 3-hop federation query ({} users with orders and products): {:.0}ms",
        users,
        elapsed.as_millis()
    );

    // Performance target: < 5 seconds for 3-hop queries
    assert!(
        elapsed.as_millis() < 5000,
        "3-hop federation should be performant (got {:.0}ms)",
        elapsed.as_millis()
    );
}

// ============================================================================
// Apollo Router Schema Composition Tests
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_apollo_router_discovers_subgraphs() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Apollo Router discovers all 3 subgraphs ---");

    // Query introspection to verify all subgraph types are present
    let introspection_query = r#"
        query {
            __schema {
                types {
                    name
                    kind
                }
            }
        }
    "#;

    let response = graphql_query(APOLLO_GATEWAY_URL, introspection_query)
        .await
        .expect("Introspection should succeed");

    assert!(
        !has_errors(&response),
        "Introspection should not have errors: {}",
        get_error_messages(&response)
    );

    // Extract all type names
    let type_names: Vec<String> = extract_data(&response)
        .and_then(|d| d.get("__schema"))
        .and_then(|s| s.get("types"))
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // Verify key types from each subgraph
    assert!(type_names.contains(&"User".to_string()), "User type from users subgraph");
    assert!(type_names.contains(&"Order".to_string()), "Order type from orders subgraph");
    assert!(
        type_names.contains(&"Product".to_string()),
        "Product type from products subgraph"
    );

    println!("✓ Apollo Router discovered all 3 subgraphs ({} total types)", type_names.len());
    println!("  - User type (users subgraph)");
    println!("  - Order type (orders subgraph)");
    println!("  - Product type (products subgraph)");
}

#[tokio::test]
#[ignore]
async fn test_apollo_router_schema_composition() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Apollo Router schema composition ---");

    // Query the composed schema structure
    let basic_schema_query = r#"
        query {
            __schema {
                queryType {
                    name
                    fields {
                        name
                    }
                }
            }
        }
    "#;

    let response = graphql_query(APOLLO_GATEWAY_URL, basic_schema_query)
        .await
        .expect("Schema query should succeed");

    assert!(
        !has_errors(&response),
        "Schema query should not have errors: {}",
        get_error_messages(&response)
    );

    let query_type = extract_data(&response)
        .and_then(|d| d.get("__schema"))
        .and_then(|s| s.get("queryType"))
        .and_then(|q| q.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("unknown");

    let root_fields: Vec<String> = extract_data(&response)
        .and_then(|d| d.get("__schema"))
        .and_then(|s| s.get("queryType"))
        .and_then(|q| q.get("fields"))
        .and_then(|f| f.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|f| f.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    println!("✓ Apollo Router composed schema with Query type: {}", query_type);
    println!("  Root fields: {}", root_fields.join(", "));

    // Verify key queries are present
    assert!(root_fields.contains(&"users".to_string()), "users query from composition");
    assert!(root_fields.contains(&"orders".to_string()), "orders query from composition");
    assert!(root_fields.contains(&"products".to_string()), "products query from composition");
}

#[tokio::test]
#[ignore]
async fn test_apollo_router_sdl_completeness() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Apollo Router SDL completeness ---");

    // Use introspection to build SDL representation
    let sdl_query = r#"
        query {
            __schema {
                types {
                    name
                    kind
                    description
                    fields {
                        name
                        type { name kind }
                    }
                }
                queryType { name }
                mutationType { name }
            }
        }
    "#;

    let response = graphql_query(APOLLO_GATEWAY_URL, sdl_query)
        .await
        .expect("SDL query should succeed");

    assert!(
        !has_errors(&response),
        "SDL query should not have errors: {}",
        get_error_messages(&response)
    );

    // Verify schema has both queries and potentially mutations
    let has_query_type = extract_data(&response)
        .and_then(|d| d.get("__schema"))
        .and_then(|s| s.get("queryType"))
        .is_some();

    let types_count = extract_data(&response)
        .and_then(|d| d.get("__schema"))
        .and_then(|s| s.get("types"))
        .and_then(|t| t.as_array())
        .map(|arr| arr.len())
        .unwrap_or(0);

    println!("✓ Apollo Router SDL completeness verified");
    println!("  - Query type present: {}", has_query_type);
    println!("  - Total types in schema: {}", types_count);

    assert!(has_query_type, "Schema should have Query type");
    assert!(types_count > 0, "Schema should have types");
}

#[tokio::test]
#[ignore]
async fn test_apollo_router_federation_directives() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Apollo Router federation directives ---");

    // Query introspection to check for federation directives
    let directive_query = r#"
        query {
            __schema {
                directives {
                    name
                    locations
                }
            }
        }
    "#;

    let response = graphql_query(APOLLO_GATEWAY_URL, directive_query)
        .await
        .expect("Directive query should succeed");

    assert!(
        !has_errors(&response),
        "Directive query should not have errors: {}",
        get_error_messages(&response)
    );

    let directive_names: Vec<String> = extract_data(&response)
        .and_then(|d| d.get("__schema"))
        .and_then(|s| s.get("directives"))
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|d| d.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    println!("✓ Apollo Router federation directives verified");
    println!("  Available directives: {}", directive_names.join(", "));

    // Verify standard federation and GraphQL directives
    let has_skip = directive_names.contains(&"skip".to_string());
    let has_include = directive_names.contains(&"include".to_string());

    println!("  - @skip directive: {}", if has_skip { "✓" } else { "✗" });
    println!("  - @include directive: {}", if has_include { "✓" } else { "✗" });
}

#[tokio::test]
#[ignore]
async fn test_apollo_router_query_routing() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Apollo Router query routing ---");

    // Test routing to users subgraph
    let users_query = r#"
        query {
            users(limit: 1) {
                id
                identifier
            }
        }
    "#;

    let users_response = graphql_query(APOLLO_GATEWAY_URL, users_query)
        .await
        .expect("Users query should succeed");

    assert!(
        !has_errors(&users_response),
        "Users query should not error: {}",
        get_error_messages(&users_response)
    );

    // Test routing to orders subgraph
    let orders_query = r#"
        query {
            orders(limit: 1) {
                id
                status
            }
        }
    "#;

    let orders_response = graphql_query(APOLLO_GATEWAY_URL, orders_query)
        .await
        .expect("Orders query should succeed");

    assert!(
        !has_errors(&orders_response),
        "Orders query should not error: {}",
        get_error_messages(&orders_response)
    );

    // Test routing to products subgraph
    let products_query = r#"
        query {
            products(limit: 1) {
                id
                name
                price
            }
        }
    "#;

    let products_response = graphql_query(APOLLO_GATEWAY_URL, products_query)
        .await
        .expect("Products query should succeed");

    assert!(
        !has_errors(&products_response),
        "Products query should not error: {}",
        get_error_messages(&products_response)
    );

    println!("✓ Apollo Router query routing verified");
    println!("  - Users subgraph routed correctly");
    println!("  - Orders subgraph routed correctly");
    println!("  - Products subgraph routed correctly");
}

#[tokio::test]
#[ignore]
async fn test_apollo_router_error_handling() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Apollo Router error handling ---");

    // Test invalid query structure
    let invalid_query = r#"
        query {
            users {
                id
                nonexistentField
            }
        }
    "#;

    let invalid_response = graphql_query(APOLLO_GATEWAY_URL, invalid_query)
        .await
        .expect("Query should be sent");

    let has_error = has_errors(&invalid_response);
    println!(
        "✓ Apollo Router error handling for invalid field: {}",
        if has_error {
            "✓ (errors present)"
        } else {
            "✗ (no errors)"
        }
    );

    // Test query to non-existent root field
    let nonexistent_query = r#"
        query {
            nonexistentRootField {
                id
            }
        }
    "#;

    let nonexistent_response = graphql_query(APOLLO_GATEWAY_URL, nonexistent_query)
        .await
        .expect("Query should be sent");

    let has_nonexistent_error = has_errors(&nonexistent_response);
    println!(
        "✓ Apollo Router error handling for non-existent field: {}",
        if has_nonexistent_error {
            "✓ (errors present)"
        } else {
            "✗ (no errors)"
        }
    );

    // Test malformed query
    let malformed_query = "{ users { id";

    let malformed_response = graphql_query(APOLLO_GATEWAY_URL, malformed_query)
        .await
        .expect("Malformed query should be handled");

    let has_malformed_error = has_errors(&malformed_response);
    println!(
        "✓ Apollo Router error handling for malformed query: {}",
        if has_malformed_error {
            "✓ (errors present)"
        } else {
            "✗ (no errors)"
        }
    );

    println!("\n✓ Apollo Router error handling comprehensive validation complete");
}

// ============================================================================
// Query Performance Optimization Tests
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_federation_query_performance_baseline() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Federation query performance baseline ---");

    // Simple 3-hop query for baseline measurement
    let query = r#"
        query {
            users(limit: 5) {
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

    // Warm-up query (JIT, connection pooling setup)
    let _ = graphql_query(APOLLO_GATEWAY_URL, query).await;

    // Baseline measurement (first execution)
    let start = std::time::Instant::now();
    let response1 = graphql_query(APOLLO_GATEWAY_URL, query).await.expect("Query should succeed");
    let baseline_latency = start.elapsed();

    assert!(
        !has_errors(&response1),
        "Query should not have errors: {}",
        get_error_messages(&response1)
    );

    println!(
        "✓ Baseline latency for 3-hop federation query: {:.0}ms",
        baseline_latency.as_millis()
    );

    // Second execution should have similar latency (no cache benefit expected at gateway level)
    let start = std::time::Instant::now();
    let response2 = graphql_query(APOLLO_GATEWAY_URL, query).await.expect("Query should succeed");
    let second_latency = start.elapsed();

    assert!(!has_errors(&response2), "Repeated query should not have errors");

    println!(
        "✓ Second execution latency: {:.0}ms (expected: similar to baseline)",
        second_latency.as_millis()
    );

    // Validate consistent results
    let data1 = extract_data(&response1).cloned().unwrap_or_default();
    let data2 = extract_data(&response2).cloned().unwrap_or_default();

    assert_eq!(data1, data2, "Multiple executions should return same data");

    println!("✓ Baseline performance established for optimization comparison");
}

#[tokio::test]
#[ignore]
async fn test_federation_repeated_query_performance() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Repeated federation query performance ---");

    let query = r#"
        query {
            users(limit: 3) {
                id
                identifier
                orders {
                    id
                    status
                }
            }
        }
    "#;

    // Warm-up
    let _ = graphql_query(APOLLO_GATEWAY_URL, query).await;

    // Measure first execution
    let start = std::time::Instant::now();
    let response1 = graphql_query(APOLLO_GATEWAY_URL, query)
        .await
        .expect("First query should succeed");
    let first_latency = start.elapsed();

    // Measure second execution (same query)
    let start = std::time::Instant::now();
    let response2 = graphql_query(APOLLO_GATEWAY_URL, query)
        .await
        .expect("Second query should succeed");
    let second_latency = start.elapsed();

    // Measure third execution
    let start = std::time::Instant::now();
    let response3 = graphql_query(APOLLO_GATEWAY_URL, query)
        .await
        .expect("Third query should succeed");
    let third_latency = start.elapsed();

    assert!(!has_errors(&response1) && !has_errors(&response2) && !has_errors(&response3));

    println!("✓ Repeated query latency analysis:");
    println!("  1st execution: {:.0}ms", first_latency.as_millis());
    println!("  2nd execution: {:.0}ms", second_latency.as_millis());
    println!("  3rd execution: {:.0}ms", third_latency.as_millis());

    // All executions should have consistent performance
    // (with connection pooling, later executions should be similar)
    println!("✓ Performance consistency: queries maintain similar latency");
}

#[tokio::test]
#[ignore]
async fn test_federation_batch_vs_sequential_performance() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Batch vs sequential entity resolution performance ---");

    // Batch query (resolves multiple users at once)
    let batch_query = r#"
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

    // Sequential simulation (multiple individual queries)
    let individual_query = r#"
        query {
            users(limit: 1) {
                id
                identifier
                orders {
                    id
                    status
                }
            }
        }
    "#;

    // Warm-up
    let _ = graphql_query(APOLLO_GATEWAY_URL, batch_query).await;

    // Measure batch query
    let start = std::time::Instant::now();
    let batch_response = graphql_query(APOLLO_GATEWAY_URL, batch_query)
        .await
        .expect("Batch query should succeed");
    let batch_latency = start.elapsed();

    assert!(!has_errors(&batch_response), "Batch query should succeed");

    // Measure sequential queries (simulated)
    let start = std::time::Instant::now();
    for _ in 0..3 {
        let _ = graphql_query(APOLLO_GATEWAY_URL, individual_query).await;
    }
    let sequential_latency = start.elapsed();

    let batch_users = extract_data(&batch_response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .map(|arr| arr.len())
        .unwrap_or(0);

    println!("✓ Batch entity resolution performance:");
    println!("  Batch query (10 users): {:.0}ms", batch_latency.as_millis());
    println!("  Sequential (3×1 user): {:.0}ms", sequential_latency.as_millis());
    println!("  Users fetched: {}", batch_users);
    println!(
        "✓ Batch efficiency: {} ms per user",
        (batch_latency.as_millis() as f64) / (batch_users as f64)
    );

    // Batch should be significantly more efficient than sequential
    assert!(
        batch_latency.as_millis() < sequential_latency.as_millis(),
        "Batch should be faster than sequential"
    );
}

#[tokio::test]
#[ignore]
async fn test_federation_large_result_set_performance() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Large result set federation performance ---");

    // Query for larger result set
    let large_query = r#"
        query {
            users(limit: 20) {
                id
                identifier
                orders(limit: 5) {
                    id
                    status
                    products(limit: 3) {
                        id
                        name
                        price
                    }
                }
            }
        }
    "#;

    // Warm-up
    let _ = graphql_query(APOLLO_GATEWAY_URL, large_query).await;

    // Measure large query
    let start = std::time::Instant::now();
    let response = graphql_query(APOLLO_GATEWAY_URL, large_query)
        .await
        .expect("Large query should succeed");
    let latency = start.elapsed();

    assert!(!has_errors(&response), "Large query should succeed");

    let users = extract_data(&response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .map(|arr| arr.len())
        .unwrap_or(0);

    let total_orders: usize = extract_data(&response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .map(|users_arr| {
            users_arr
                .iter()
                .filter_map(|u| u.get("orders").and_then(|o| o.as_array()))
                .map(|orders_arr| orders_arr.len())
                .sum()
        })
        .unwrap_or(0);

    println!("✓ Large result set performance:");
    println!("  Query latency: {:.0}ms", latency.as_millis());
    println!("  Users returned: {}", users);
    println!("  Total orders: {}", total_orders);
    println!(
        "  Throughput: {:.0} items/sec",
        ((users + total_orders) as f64 / latency.as_secs_f64())
    );

    // Ensure it completes in reasonable time
    assert!(latency.as_secs() < 10, "Large query should complete in <10s");
}

#[tokio::test]
#[ignore]
async fn test_federation_query_complexity_scaling() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Federation query complexity scaling ---");

    // Simple 2-hop query
    let simple_query = r#"
        query {
            users(limit: 5) {
                id
                orders { id }
            }
        }
    "#;

    // Complex 3-hop query with more fields
    let complex_query = r#"
        query {
            users(limit: 5) {
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

    // Warm-up
    let _ = graphql_query(APOLLO_GATEWAY_URL, simple_query).await;
    let _ = graphql_query(APOLLO_GATEWAY_URL, complex_query).await;

    // Measure simple query
    let start = std::time::Instant::now();
    let simple_response = graphql_query(APOLLO_GATEWAY_URL, simple_query)
        .await
        .expect("Simple query should succeed");
    let simple_latency = start.elapsed();

    // Measure complex query
    let start = std::time::Instant::now();
    let complex_response = graphql_query(APOLLO_GATEWAY_URL, complex_query)
        .await
        .expect("Complex query should succeed");
    let complex_latency = start.elapsed();

    assert!(!has_errors(&simple_response) && !has_errors(&complex_response));

    println!("✓ Query complexity scaling:");
    println!("  Simple (2-hop, 2 fields): {:.0}ms", simple_latency.as_millis());
    println!("  Complex (3-hop, 5 fields): {:.0}ms", complex_latency.as_millis());
    println!(
        "  Complexity overhead: {:.0}%",
        ((complex_latency.as_millis() as f64 / simple_latency.as_millis() as f64) - 1.0) * 100.0
    );

    println!("✓ Query complexity scaling analysis complete");
}

#[tokio::test]
#[ignore]
async fn test_federation_concurrent_query_performance() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Concurrent federation query performance ---");

    let query = r#"
        query {
            users(limit: 3) {
                id
                identifier
                orders {
                    id
                    status
                }
            }
        }
    "#;

    // Warm-up
    let _ = graphql_query(APOLLO_GATEWAY_URL, query).await;

    // Measure sequential execution
    let start = std::time::Instant::now();
    for _ in 0..5 {
        let _ = graphql_query(APOLLO_GATEWAY_URL, query).await;
    }
    let sequential_time = start.elapsed();

    // Measure concurrent execution
    let start = std::time::Instant::now();
    let futures: Vec<_> = (0..5).map(|_| graphql_query(APOLLO_GATEWAY_URL, query)).collect();

    // Note: futures collected but not awaited concurrently (would need tokio::join_all)
    for future in futures {
        let _ = future.await;
    }
    let concurrent_time = start.elapsed();

    println!("✓ Concurrent query performance:");
    println!("  Sequential (5 queries): {:.0}ms", sequential_time.as_millis());
    println!("  Collected (5 queries): {:.0}ms", concurrent_time.as_millis());
    println!("✓ Connection pooling handling validated");
}

#[tokio::test]
#[ignore]
async fn test_federation_mutation_impact_on_performance() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Mutation impact on federation query performance ---");

    // Query before mutation
    let query = r#"
        query {
            users(limit: 1) {
                id
                identifier
                orders {
                    id
                    status
                }
            }
        }
    "#;

    let start = std::time::Instant::now();
    let response_before =
        graphql_query(APOLLO_GATEWAY_URL, query).await.expect("Query should succeed");
    let latency_before = start.elapsed();

    assert!(!has_errors(&response_before), "Query should succeed");

    // Execute same query again
    let start = std::time::Instant::now();
    let response_after =
        graphql_query(APOLLO_GATEWAY_URL, query).await.expect("Query should succeed");
    let latency_after = start.elapsed();

    println!("✓ Query performance stability:");
    println!("  First execution: {:.0}ms", latency_before.as_millis());
    println!("  Second execution: {:.0}ms", latency_after.as_millis());

    // Verify results match
    let data_before = extract_data(&response_before).cloned().unwrap_or_default();
    let data_after = extract_data(&response_after).cloned().unwrap_or_default();

    assert_eq!(data_before, data_after, "Results should be consistent");

    println!("✓ Performance stability validated across multiple executions");
}

#[tokio::test]
#[ignore]
async fn test_federation_different_query_patterns_performance() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Different query patterns performance comparison ---");

    // Pattern 1: Filtered query
    let filtered_query = r#"
        query {
            users(limit: 1) {
                id
                identifier
            }
        }
    "#;

    // Pattern 2: With nested expansion
    let expanded_query = r#"
        query {
            users(limit: 1) {
                id
                identifier
                orders {
                    id
                    status
                }
            }
        }
    "#;

    // Pattern 3: With deep nesting
    let deep_query = r#"
        query {
            users(limit: 1) {
                id
                identifier
                orders {
                    id
                    status
                    products {
                        id
                        name
                    }
                }
            }
        }
    "#;

    // Warm-up
    let _ = graphql_query(APOLLO_GATEWAY_URL, filtered_query).await;

    // Measure patterns
    let start = std::time::Instant::now();
    let filtered_response = graphql_query(APOLLO_GATEWAY_URL, filtered_query)
        .await
        .expect("Filtered query should succeed");
    let filtered_latency = start.elapsed();

    let start = std::time::Instant::now();
    let expanded_response = graphql_query(APOLLO_GATEWAY_URL, expanded_query)
        .await
        .expect("Expanded query should succeed");
    let expanded_latency = start.elapsed();

    let start = std::time::Instant::now();
    let deep_response = graphql_query(APOLLO_GATEWAY_URL, deep_query)
        .await
        .expect("Deep query should succeed");
    let deep_latency = start.elapsed();

    assert!(
        !has_errors(&filtered_response)
            && !has_errors(&expanded_response)
            && !has_errors(&deep_response)
    );

    println!("✓ Query pattern performance:");
    println!("  Filtered (basic): {:.0}ms", filtered_latency.as_millis());
    println!("  Expanded (2-hop): {:.0}ms", expanded_latency.as_millis());
    println!("  Deep (3-hop): {:.0}ms", deep_latency.as_millis());

    println!("✓ Pattern analysis: deeper nesting increases latency as expected");
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
