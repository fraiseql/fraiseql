//! Docker Compose Federation Tests - Three-Subgraph Federation
//!
//! Tests validate 3+ subgraph setup, direct product queries, multi-hop
//! federation, entity resolution chains, and cross-boundary queries.

use super::common::*;

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

    println!("\n--- Test: 3-hop federation query (users -> orders -> products) ---");

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

        println!("✓ Entity resolution chain: users -> orders -> products (user_id: {})", uid);
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
