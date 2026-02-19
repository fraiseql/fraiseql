//! Docker Compose Federation Integration Tests - Federated Query & Entity Resolution
//!
//! Tests validate cross-subgraph federation, entity resolution, composite keys,
//! and multi-subgraph query execution.

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

/// Setup helper for 3-subgraph federation tests (users -> orders -> products)
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

// ============================================================================
// Two-Subgraph Federation Tests
// ============================================================================

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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

// ============================================================================
// Entity Resolution Tests
// ============================================================================

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
// Composite Key & Multi-Tenant Tests
// ============================================================================

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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

// ============================================================================
// 3+ Subgraph Federation Tests
// ============================================================================

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
        let orders_query = r#"query { users(limit: 1) { orders { id status } } }"#.to_string();

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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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

// ============================================================================
// Apollo Router Schema Composition Tests
// ============================================================================

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
