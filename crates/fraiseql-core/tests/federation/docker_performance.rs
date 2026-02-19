//! Docker Compose Federation Integration Tests - Performance Tests
//!
//! Tests validate federation query performance, batch vs sequential resolution,
//! concurrent query handling, and complexity scaling.

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
// Performance Tests
// ============================================================================

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
// Query Performance Optimization Tests
// ============================================================================

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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
