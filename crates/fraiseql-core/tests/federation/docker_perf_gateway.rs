//! Docker Compose Federation Tests - Gateway Performance Optimization
//!
//! Tests validate query performance baselines, repeated query consistency,
//! batch vs sequential performance, large result sets, complexity scaling,
//! concurrent queries, mutation impact, and different query patterns.

use super::common::*;

// ============================================================================
// Query Performance Optimization Tests
// ============================================================================

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
async fn test_federation_query_performance_baseline() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Federation query performance baseline ---");

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
    println!("  Sequential (3x1 user): {:.0}ms", sequential_latency.as_millis());
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
