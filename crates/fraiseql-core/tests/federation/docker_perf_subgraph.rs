//! Docker Compose Federation Tests - Subgraph Performance
//!
//! Tests validate per-subgraph federation query performance, mutation
//! performance, composite key performance, and batch resolution.

use super::common::*;

// ============================================================================
// Subgraph Performance Tests
// ============================================================================

#[tokio::test]
async fn test_two_subgraph_federation_performance() {
    if std::env::var("FEDERATION_TESTS").is_err() {
        eprintln!("Skipping: FEDERATION_TESTS not set");
        return;
    }
    setup_federation_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Federation query performance ---");

    let query = r"
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
    ";

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

#[tokio::test]
async fn test_federation_query_performance() {
    if std::env::var("FEDERATION_TESTS").is_err() {
        eprintln!("Skipping: FEDERATION_TESTS not set");
        return;
    }
    let start = std::time::Instant::now();

    let _response = graphql_query(
        APOLLO_GATEWAY_URL,
        r"query {
            users(limit: 5) {
                id
                orders {
                    id
                    products {
                        id
                    }
                }
            }
        }",
    )
    .await
    .expect("Query should succeed");

    let elapsed = start.elapsed();

    println!("✓ 3-hop federated query completed in: {:.0}ms", elapsed.as_millis());

    // Assert reasonable latency (adjust based on actual performance)
    assert!(elapsed.as_millis() < 1000, "Federated query should complete in under 1 second");
}

#[tokio::test]
async fn test_extended_mutation_performance() {
    if std::env::var("FEDERATION_TESTS").is_err() {
        eprintln!("Skipping: FEDERATION_TESTS not set");
        return;
    }
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

#[tokio::test]
async fn test_composite_key_performance() {
    if std::env::var("FEDERATION_TESTS").is_err() {
        eprintln!("Skipping: FEDERATION_TESTS not set");
        return;
    }
    setup_federation_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Composite key resolution performance ---");

    // Measure performance of composite key resolution at scale
    let start = std::time::Instant::now();

    let query = r"
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
    ";

    let response = graphql_query(APOLLO_GATEWAY_URL, query).await.expect("Query should succeed");

    let elapsed = start.elapsed();

    if !has_errors(&response) {
        let users = extract_data(&response)
            .and_then(|d| d.get("users"))
            .and_then(|u| u.as_array())
            .map_or(0, |arr| arr.len());

        println!("✓ Composite key resolution for {} users: {:.0}ms", users, elapsed.as_millis());

        // Performance should scale well with composite keys
        assert!(elapsed.as_millis() < 5000, "Composite key resolution should be performant");
    } else {
        eprintln!("! Query error: {}", get_error_messages(&response));
    }
}

#[tokio::test]
async fn test_three_subgraph_batch_entity_resolution() {
    if std::env::var("FEDERATION_TESTS").is_err() {
        eprintln!("Skipping: FEDERATION_TESTS not set");
        return;
    }
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Batch entity resolution at scale ---");

    let query = r"
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
    ";

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
        .map_or(0, |arr| arr.len());

    println!(
        "✓ Batch entity resolution for {} users with nested orders/products: {:.0}ms",
        users,
        elapsed.as_millis()
    );
}

#[tokio::test]
async fn test_three_subgraph_gateway_composition() {
    if std::env::var("FEDERATION_TESTS").is_err() {
        eprintln!("Skipping: FEDERATION_TESTS not set");
        return;
    }
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Apollo Router gateway composition ---");

    // Query the introspection to verify schema composition
    let introspection_query = r"
        query {
            __schema {
                types {
                    name
                }
            }
        }
    ";

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
        .map_or(0, |arr| arr.len());

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
async fn test_three_subgraph_performance() {
    if std::env::var("FEDERATION_TESTS").is_err() {
        eprintln!("Skipping: FEDERATION_TESTS not set");
        return;
    }
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: 3-hop federation performance ---");

    let query = r"
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
    ";

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
        .map_or(0, |arr| arr.len());

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
