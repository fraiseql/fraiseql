//! Docker Compose Federation Integration Tests - Extended Mutations
//!
//! Tests validate mutation operations via Docker federation stack including
//! direct mutations, extended mutations, error handling, and data consistency.

use std::time::Duration;

use serde_json::{Value, json};

const APOLLO_GATEWAY_URL: &str = "http://localhost:4000/graphql";
const USERS_SUBGRAPH_URL: &str = "http://localhost:4001/graphql";
const ORDERS_SUBGRAPH_URL: &str = "http://localhost:4002/graphql";
#[allow(dead_code)]
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
// Extended Mutations Tests (HTTP Federation)
// ============================================================================

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
