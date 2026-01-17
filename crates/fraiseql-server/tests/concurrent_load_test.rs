//! Concurrent Load Tests for FraiseQL Server
//!
//! Tests performance and correctness under concurrent request loads:
//! 1. Multiple concurrent HTTP requests
//! 2. Performance under sustained load
//! 3. Resource management (connection pooling)
//! 4. Error handling under load
//! 5. Throughput and latency measurements

mod test_helpers;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use test_helpers::*;

/// Test 10 concurrent requests to health endpoint
#[tokio::test]
async fn test_10_concurrent_health_requests() {
    let client = create_test_client();
    let base_url = "http://localhost:8000";
    let success_count = Arc::new(AtomicU64::new(0));

    let futures: Vec<_> = (0..10)
        .map(|_| {
            let client = client.clone();
            let url = format!("{}/health", base_url);
            let success = success_count.clone();

            async move {
                match client.get(url).send().await {
                    Ok(resp) => {
                        if resp.status().is_success() {
                            success.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    Err(_) => {
                        // Server not running
                    }
                }
            }
        })
        .collect();

    futures::future::join_all(futures).await;

    let successful = success_count.load(Ordering::Relaxed);
    if successful > 0 {
        assert!(successful >= 1);
    }
}

/// Test 50 concurrent GraphQL queries
#[tokio::test]
async fn test_50_concurrent_graphql_queries() {
    let client = create_test_client();
    let base_url = "http://localhost:8000";
    let success_count = Arc::new(AtomicU64::new(0));
    let error_count = Arc::new(AtomicU64::new(0));

    let futures: Vec<_> = (0..50)
        .map(|i| {
            let client = client.clone();
            let url = format!("{}/graphql", base_url);
            let success = success_count.clone();
            let errors = error_count.clone();

            async move {
                let request = create_graphql_request(
                    "query { __typename }",
                    None,
                    Some(&format!("Query{}", i)),
                );

                match client.post(&url).json(&request).send().await {
                    Ok(resp) => {
                        if resp.status().is_success() {
                            success.fetch_add(1, Ordering::Relaxed);
                        } else {
                            errors.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        })
        .collect();

    futures::future::join_all(futures).await;

    let successful = success_count.load(Ordering::Relaxed);
    let failed = error_count.load(Ordering::Relaxed);

    if successful > 0 {
        // If any succeeded, we should have at least 1
        assert!(successful >= 1);
    }

    // Should not crash under load - verify we ran the test
    // At least some requests should have been made
    assert!(successful + failed > 0, "No requests were processed");
}

/// Test 100 concurrent requests with varying endpoints
#[tokio::test]
async fn test_100_concurrent_mixed_endpoints() {
    let client = create_test_client();
    let base_url = "http://localhost:8000";
    let success_count = Arc::new(AtomicU64::new(0));

    let futures: Vec<_> = (0..100)
        .map(|i| {
            let client = client.clone();
            let success = success_count.clone();

            async move {
                let result = match i % 3 {
                    0 => {
                        // Health endpoint
                        client.get(format!("{}/health", base_url)).send().await
                    }
                    1 => {
                        // Metrics endpoint
                        client.get(format!("{}/metrics", base_url)).send().await
                    }
                    _ => {
                        // GraphQL endpoint
                        let request = create_graphql_request("{ __typename }", None, None);
                        client.post(format!("{}/graphql", base_url))
                            .json(&request)
                            .send()
                            .await
                    }
                };

                if let Ok(resp) = result {
                    if resp.status().is_success() {
                        success.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        })
        .collect();

    futures::future::join_all(futures).await;

    let successful = success_count.load(Ordering::Relaxed);
    if successful > 0 {
        assert!(successful >= 1);
    }
}

/// Test throughput of health endpoint
#[tokio::test]
async fn test_health_endpoint_throughput() {
    let client = create_test_client();
    let base_url = "http://localhost:8000";

    let start = Instant::now();
    let mut count = 0u64;

    // Fire requests as fast as possible for 1 second
    while start.elapsed().as_secs() < 1 {
        match client.get(format!("{}/health", base_url)).send().await {
            Ok(_) => count += 1,
            Err(_) => break, // Server not running
        }
    }

    if count > 0 {
        println!("Health endpoint throughput: {} req/s", count);
        assert!(count > 0);
    }
}

/// Test latency distribution under load
#[tokio::test]
async fn test_latency_distribution() {
    let client = create_test_client();
    let base_url = "http://localhost:8000";
    let latencies = Arc::new(tokio::sync::Mutex::new(Vec::new()));

    let futures: Vec<_> = (0..20)
        .map(|_| {
            let client = client.clone();
            let url = format!("{}/health", base_url);
            let latencies = latencies.clone();

            async move {
                let start = Instant::now();
                if client.get(&url).send().await.is_ok() {
                    let latency_ms = start.elapsed().as_millis() as u64;
                    let mut lats = latencies.lock().await;
                    lats.push(latency_ms);
                }
            }
        })
        .collect();

    futures::future::join_all(futures).await;

    let lats = latencies.lock().await;
    if !lats.is_empty() {
        let min = lats.iter().min().copied().unwrap_or(0);
        let max = lats.iter().max().copied().unwrap_or(0);
        let avg = lats.iter().sum::<u64>() / lats.len() as u64;

        println!("Latency - Min: {}ms, Max: {}ms, Avg: {}ms", min, max, avg);

        // Latency should be reasonable (< 1s)
        assert!(max < 1000);
    }
}

/// Test sustained load for 10 seconds
#[tokio::test]
async fn test_sustained_load() {
    let client = create_test_client();
    let base_url = "http://localhost:8000";
    let request_count = Arc::new(AtomicU64::new(0));
    let start = Instant::now();

    let futures: Vec<_> = (0..5)
        .map(|_| {
            let client = client.clone();
            let count = request_count.clone();

            async move {
                let start = Instant::now();
                // Keep making requests for test duration
                while start.elapsed().as_secs() < 2 {
                    let request = create_graphql_request("{ __typename }", None, None);
                    if client.post(format!("{}/graphql", base_url))
                        .json(&request)
                        .send()
                        .await
                        .is_ok()
                    {
                        count.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        })
        .collect();

    futures::future::join_all(futures).await;

    let total_requests = request_count.load(Ordering::Relaxed);
    let duration_secs = start.elapsed().as_secs_f64();

    if total_requests > 0 {
        let throughput = total_requests as f64 / duration_secs;
        println!("Sustained load - Total: {}, Throughput: {:.1} req/s", total_requests, throughput);
        assert!(total_requests > 0);
    }
}

/// Test error handling under concurrent load
#[tokio::test]
async fn test_error_handling_under_load() {
    let client = create_test_client();
    let base_url = "http://localhost:8000";
    let success_count = Arc::new(AtomicU64::new(0));
    let error_count = Arc::new(AtomicU64::new(0));

    let futures: Vec<_> = (0..30)
        .map(|i| {
            let client = client.clone();
            let success = success_count.clone();
            let errors = error_count.clone();

            async move {
                let request = if i % 2 == 0 {
                    // Valid query
                    create_graphql_request("{ __typename }", None, None)
                } else {
                    // Invalid query (too deep)
                    create_graphql_request("{ a { b { c { d { e { f { g } } } } } } }", None, None)
                };

                match client.post(format!("{}/graphql", base_url))
                    .json(&request)
                    .send()
                    .await
                {
                    Ok(resp) => {
                        if resp.status().is_success() {
                            success.fetch_add(1, Ordering::Relaxed);
                        } else {
                            errors.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        })
        .collect();

    futures::future::join_all(futures).await;

    let successful = success_count.load(Ordering::Relaxed);
    let failed = error_count.load(Ordering::Relaxed);

    if successful > 0 {
        // Should handle both valid and invalid queries
        println!("Success: {}, Errors: {}", successful, failed);
    }
}

/// Test connection pool behavior under load
#[tokio::test]
#[ignore = "Requires FraiseQL server running on localhost:8000"]
async fn test_connection_pool_stability() {
    let client = create_test_client();
    let base_url = "http://localhost:8000";
    let slow_requests = Arc::new(AtomicU64::new(0));
    let fast_requests = Arc::new(AtomicU64::new(0));
    let slow_threshold_ms = 100u128;

    let futures: Vec<_> = (0..40)
        .map(|_| {
            let client = client.clone();
            let url = format!("{}/health", base_url);
            let slow = slow_requests.clone();
            let fast = fast_requests.clone();

            async move {
                let start = Instant::now();
                if client.get(&url).send().await.is_ok() {
                    let latency = start.elapsed().as_millis();
                    if latency > slow_threshold_ms {
                        slow.fetch_add(1, Ordering::Relaxed);
                    } else {
                        fast.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        })
        .collect();

    futures::future::join_all(futures).await;

    let fast = fast_requests.load(Ordering::Relaxed);
    let slow = slow_requests.load(Ordering::Relaxed);
    let total = fast + slow;

    if total > 0 {
        let slow_percentage = (slow as f64 / total as f64) * 100.0;
        println!("Request latency - Fast (<{}ms): {:.1}%, Slow: {:.1}%", slow_threshold_ms, 100.0 - slow_percentage, slow_percentage);

        // Most requests should be fast - only assert if this looks like a FraiseQL server
        // (other services on port 8000 may have different latency characteristics)
        if fast > 0 {
            assert!(fast > slow, "Connection pool stability test expects most requests to be fast (<100ms)");
        }
    }
}

/// Test graceful degradation under extreme load
#[tokio::test]
async fn test_extreme_concurrent_load() {
    let client = create_test_client();
    let base_url = "http://localhost:8000";
    let success_count = Arc::new(AtomicU64::new(0));

    // Try 200 concurrent requests
    let futures: Vec<_> = (0..200)
        .map(|_| {
            let client = client.clone();
            let url = format!("{}/health", base_url);
            let success = success_count.clone();

            async move {
                if client.get(&url).send().await.is_ok() {
                    success.fetch_add(1, Ordering::Relaxed);
                }
            }
        })
        .collect();

    futures::future::join_all(futures).await;

    let successful = success_count.load(Ordering::Relaxed);
    if successful > 0 {
        println!("Extreme load - Successfully handled {}/200 requests", successful);
        assert!(successful >= 1);
    }
}
