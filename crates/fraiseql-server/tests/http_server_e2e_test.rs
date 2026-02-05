//! HTTP Server End-to-End Tests
//!
//! Tests complete HTTP request/response flow:
//! 1. HTTP server starts and binds to port
//! 2. Client makes HTTP requests
//! 3. Server processes requests (parsing, validation, execution)
//! 4. Responses returned in correct format
//! 5. Error handling works correctly
//!
//! These are TRUE E2E tests with actual HTTP server running.
//!
//! ## Running Tests
//!
//! By default, tests connect to `http://localhost:8000`. Set the
//! `FRAISEQL_TEST_URL` environment variable to test against a different server:
//!
//! ```bash
//! # Test against Docker E2E server
//! FRAISEQL_TEST_URL=http://localhost:9001 cargo test -p fraiseql-server --test http_server_e2e_test -- --include-ignored
//! ```

mod test_helpers;

use std::env;

use reqwest::StatusCode;
use test_helpers::*;

/// Get the base URL for testing. Defaults to localhost:8000, but can be
/// overridden with FRAISEQL_TEST_URL environment variable.
fn get_test_base_url() -> String {
    env::var("FRAISEQL_TEST_URL").unwrap_or_else(|_| "http://localhost:8000".to_string())
}

/// Test that health endpoint responds correctly
#[tokio::test]
#[ignore = "Requires FraiseQL server; set FRAISEQL_TEST_URL to run"]
async fn test_health_endpoint_responds() {
    let client = create_test_client();
    let base_url = get_test_base_url();

    // Test health endpoint
    let response = client.get(format!("{}/health", base_url)).send().await;

    match response {
        Ok(resp) => {
            assert_eq!(resp.status(), StatusCode::OK);
            let json = resp
                .json::<serde_json::Value>()
                .await
                .expect("health response should be valid JSON");
            assert_health_response(&json);
        },
        Err(e) => {
            eprintln!("Warning: Could not connect to server: {}", e);
        },
    }
}

/// Test that metrics endpoint responds with Prometheus format (requires bearer token)
#[tokio::test]
#[ignore = "Requires FraiseQL server; set FRAISEQL_TEST_URL to run"]
async fn test_metrics_endpoint_responds() {
    let client = create_test_client();
    let base_url = get_test_base_url();
    let token = get_metrics_token();

    let response = client
        .get(format!("{}/metrics", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await;

    match response {
        Ok(resp) => {
            assert_eq!(resp.status(), StatusCode::OK);
            let content_type = resp
                .headers()
                .get("content-type")
                .expect("metrics response should have Content-Type header");
            let ct_str = content_type.to_str().unwrap();
            assert!(
                ct_str.contains("text/plain") || ct_str.contains("application/openmetrics"),
                "metrics Content-Type should be Prometheus format, got: {ct_str}"
            );

            let text = resp.text().await.expect("metrics response should have a body");
            assert!(
                text.contains("fraiseql_graphql_queries_total"),
                "metrics body should contain Prometheus metric names"
            );
        },
        Err(e) => {
            eprintln!("Warning: Could not connect to server: {}", e);
        },
    }
}

/// Test that metrics JSON endpoint responds correctly (requires bearer token)
#[tokio::test]
#[ignore = "Requires FraiseQL server; set FRAISEQL_TEST_URL to run"]
async fn test_metrics_json_endpoint_responds() {
    let client = create_test_client();
    let base_url = get_test_base_url();
    let token = get_metrics_token();

    let response = client
        .get(format!("{}/metrics/json", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await;

    match response {
        Ok(resp) => {
            assert_eq!(resp.status(), StatusCode::OK);

            let json = resp
                .json::<serde_json::Value>()
                .await
                .expect("metrics JSON response should be valid JSON");
            assert_metrics_response(&json);
        },
        Err(e) => {
            eprintln!("Warning: Could not connect to server: {}", e);
        },
    }
}

/// Test that metrics endpoint rejects requests without token
#[tokio::test]
#[ignore = "Requires FraiseQL server; set FRAISEQL_TEST_URL to run"]
async fn test_metrics_endpoint_requires_auth() {
    let client = create_test_client();
    let base_url = get_test_base_url();

    // Request without Authorization header
    let response = client.get(format!("{}/metrics", base_url)).send().await;

    match response {
        Ok(resp) => {
            assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        },
        Err(e) => {
            eprintln!("Warning: Could not connect to server: {}", e);
        },
    }
}

/// Test that metrics endpoint rejects invalid token
#[tokio::test]
#[ignore = "Requires FraiseQL server; set FRAISEQL_TEST_URL to run"]
async fn test_metrics_endpoint_rejects_invalid_token() {
    let client = create_test_client();
    let base_url = get_test_base_url();

    let response = client
        .get(format!("{}/metrics", base_url))
        .header("Authorization", "Bearer wrong-token")
        .send()
        .await;

    match response {
        Ok(resp) => {
            assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        },
        Err(e) => {
            eprintln!("Warning: Could not connect to server: {}", e);
        },
    }
}

/// Test that invalid paths return 404
#[tokio::test]
#[ignore = "Requires FraiseQL server; set FRAISEQL_TEST_URL to run"]
async fn test_invalid_path_returns_404() {
    let client = create_test_client();
    let base_url = get_test_base_url();

    let response = client.get(format!("{}/invalid/path", base_url)).send().await;

    match response {
        Ok(resp) => {
            assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        },
        Err(e) => {
            eprintln!("Warning: Could not connect to server: {}", e);
        },
    }
}

/// Test GraphQL endpoint accepts POST requests
#[tokio::test]
#[ignore = "Requires FraiseQL server; set FRAISEQL_TEST_URL to run"]
async fn test_graphql_endpoint_accepts_post() {
    let client = create_test_client();
    let base_url = get_test_base_url();

    // Use a real query from our schema (users list query)
    let request = create_graphql_request("{ users { id name } }", None, None);

    let response = client.post(format!("{}/graphql", base_url)).json(&request).send().await;

    match response {
        Ok(resp) => {
            assert_eq!(resp.status(), StatusCode::OK);
            let json = resp
                .json::<serde_json::Value>()
                .await
                .expect("GraphQL response should be valid JSON");
            assert_graphql_response(&json);
        },
        Err(e) => {
            eprintln!("Warning: Could not connect to server: {}", e);
        },
    }
}

/// Test GraphQL endpoint rejects GET requests
#[tokio::test]
#[ignore = "Requires FraiseQL server; set FRAISEQL_TEST_URL to run"]
async fn test_graphql_endpoint_rejects_get() {
    let client = create_test_client();
    let base_url = get_test_base_url();

    let response = client.get(format!("{}/graphql", base_url)).send().await;

    match response {
        Ok(resp) => {
            // Should reject GET with 405 or similar
            assert_ne!(resp.status(), StatusCode::OK);
        },
        Err(e) => {
            eprintln!("Warning: Could not connect to server: {}", e);
        },
    }
}

/// Test response includes correct headers
#[tokio::test]
#[ignore = "Requires FraiseQL server; set FRAISEQL_TEST_URL to run"]
async fn test_response_headers_correct() {
    let client = create_test_client();
    let base_url = get_test_base_url();

    // Use a real query from our schema
    let request = create_graphql_request("{ users { id name } }", None, None);

    let response = client.post(format!("{}/graphql", base_url)).json(&request).send().await;

    match response {
        Ok(resp) => {
            // Should have Content-Type header
            let content_type = resp.headers().get("content-type");
            assert!(content_type.is_some());

            if let Some(ct) = content_type {
                let ct_str = ct.to_str().unwrap_or("");
                assert!(ct_str.contains("application/json"));
            }
        },
        Err(e) => {
            eprintln!("Warning: Could not connect to server: {}", e);
        },
    }
}

/// Test empty query returns validation error
#[tokio::test]
#[ignore = "Requires FraiseQL server; set FRAISEQL_TEST_URL to run"]
async fn test_empty_query_returns_error() {
    let client = create_test_client();
    let base_url = get_test_base_url();

    let request = create_graphql_request("", None, None);

    let response = client.post(format!("{}/graphql", base_url)).json(&request).send().await;

    match response {
        Ok(resp) => {
            // Server may return 400 Bad Request or 200 OK with errors in body
            // Both are valid ways to handle empty query
            let status = resp.status();
            assert!(
                status == StatusCode::OK || status == StatusCode::BAD_REQUEST,
                "Expected OK or BAD_REQUEST, got {}",
                status
            );

            let body = resp.json::<serde_json::Value>().await;
            if let Ok(json) = body {
                // Should have errors if 200 response
                if status == StatusCode::OK {
                    assert!(json.get("errors").is_some());
                }
            }
        },
        Err(e) => {
            eprintln!("Warning: Could not connect to server: {}", e);
        },
    }
}

/// Test malformed JSON returns bad request
#[tokio::test]
#[ignore = "Requires FraiseQL server; set FRAISEQL_TEST_URL to run"]
async fn test_malformed_json_returns_error() {
    let client = create_test_client();
    let base_url = get_test_base_url();

    let response = client
        .post(format!("{}/graphql", base_url))
        .body("{invalid json")
        .header("Content-Type", "application/json")
        .send()
        .await;

    match response {
        Ok(resp) => {
            // Should return 400 or 422
            assert!(resp.status().is_client_error());
        },
        Err(e) => {
            eprintln!("Warning: Could not connect to server: {}", e);
        },
    }
}

/// Test introspection endpoint responds
#[tokio::test]
#[ignore = "Requires FraiseQL server; set FRAISEQL_TEST_URL to run"]
async fn test_introspection_endpoint_responds() {
    let client = create_test_client();
    let base_url = get_test_base_url();

    let response = client.post(format!("{}/introspection", base_url)).send().await;

    match response {
        Ok(resp) => {
            let status = resp.status();
            assert!(
                status == StatusCode::OK
                    || status == StatusCode::BAD_REQUEST
                    || status == StatusCode::METHOD_NOT_ALLOWED,
                "Introspection should return 200, 400, or 405; got {status}"
            );
        },
        Err(e) => {
            eprintln!("Warning: Could not connect to server: {}", e);
        },
    }
}

/// Test concurrent requests to health endpoint
#[tokio::test]
#[ignore = "Requires FraiseQL server; set FRAISEQL_TEST_URL to run"]
async fn test_concurrent_health_requests() {
    let client = create_test_client();
    let base_url = get_test_base_url();

    // Create 10 concurrent requests
    let futures: Vec<_> = (0..10)
        .map(|_| {
            let client = client.clone();
            let url = format!("{}/health", base_url);
            async move { client.get(url).send().await }
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    let successful = results.iter().filter(|r| r.is_ok()).count();

    // All 10 concurrent health checks should succeed
    assert_eq!(
        successful, 10,
        "all concurrent health requests should succeed, got {successful}/10"
    );
}

/// Test response content type consistency
#[tokio::test]
#[ignore = "Requires FraiseQL server; set FRAISEQL_TEST_URL to run"]
async fn test_content_type_consistency() {
    let client = create_test_client();
    let base_url = get_test_base_url();

    // Test GraphQL endpoint with a real query
    let request = create_graphql_request("{ users { id name } }", None, None);
    let response = client.post(format!("{}/graphql", base_url)).json(&request).send().await;

    match response {
        Ok(resp) => {
            let content_type = resp
                .headers()
                .get("content-type")
                .expect("GraphQL response should have Content-Type header");
            let ct_str = content_type.to_str().unwrap();
            assert!(
                ct_str.contains("application/json"),
                "GraphQL response Content-Type should be application/json, got: {ct_str}"
            );
        },
        Err(e) => {
            eprintln!("Warning: Could not connect to server: {}", e);
        },
    }
}

// Note: These tests assume a FraiseQL server is running on localhost:8000
// In CI/CD, you would typically:
// 1. Start the server in a test harness
// 2. Run these tests against it
// 3. Tear down the server
//
// Example with test harness:
// #[tokio::test]
// async fn test_with_server() {
//     let server = TestServer::start().await;
//     let client = create_test_client();
//
//     let response = client.get(server.health_url()).send().await;
//     assert!(response.is_ok());
//
//     server.shutdown().await;
// }
