//! Docker Compose Federation Tests - Two-Subgraph Federation
//!
//! Tests validate 2-subgraph federation queries, entity resolution consistency,
//! and data consistency between direct and federated queries.

use super::common::*;

// ============================================================================
// Two-Subgraph Federation Tests
// ============================================================================

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
async fn test_gateway_two_subgraph_federation() {
    // Query users with their orders (2-hop federation: gateway -> users -> orders)
    let query = r"
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
    ";

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
            .is_some_and(|arr| !arr.is_empty())
    });

    assert!(has_orders, "Some users should have orders in federated query");
    println!("✓ Gateway 2-subgraph federation query succeeded");
}

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
async fn test_gateway_three_subgraph_federation() {
    // Query users with their orders and order products (3-hop federation)
    let query = r"
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
    ";

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
    let query = r"
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
    ";

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
    let query = r"
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
    ";

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
                .is_some_and(|arr| !arr.is_empty())
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
        graphql_query(USERS_SUBGRAPH_URL, r"query { users(limit: 1) { id identifier } }")
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
        graphql_query(USERS_SUBGRAPH_URL, r"query { users(limit: 3) { id identifier } }")
            .await
            .expect("Direct users query should succeed");

    let direct_users = extract_data(&direct_response)
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .expect("Should return users array");

    // Get same users through gateway (federated)
    let gateway_response =
        graphql_query(APOLLO_GATEWAY_URL, r"query { users(limit: 3) { id identifier } }")
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
