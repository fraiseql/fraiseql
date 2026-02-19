//! Docker Compose Federation Integration Tests - Service Health & Composition
//!
//! Tests validate service startup, health checks, and schema composition.

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
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
async fn test_users_subgraph_health() {
    if let Err(e) = wait_for_service(USERS_SUBGRAPH_URL, 30).await {
        panic!("Users subgraph health check failed: {}", e);
    }
}

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
async fn test_orders_subgraph_health() {
    if let Err(e) = wait_for_service(ORDERS_SUBGRAPH_URL, 30).await {
        panic!("Orders subgraph health check failed: {}", e);
    }
}

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
async fn test_products_subgraph_health() {
    if let Err(e) = wait_for_service(PRODUCTS_SUBGRAPH_URL, 30).await {
        panic!("Products subgraph health check failed: {}", e);
    }
}

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
async fn test_apollo_router_health() {
    if let Err(e) = wait_for_service(APOLLO_GATEWAY_URL, 30).await {
        panic!("Apollo Router health check failed: {}", e);
    }
}

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
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

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
async fn test_two_subgraph_setup_validation() {
    // Validate that 2-subgraph setup is working
    if let Err(e) = setup_federation_tests().await {
        panic!("2-subgraph federation setup failed: {}", e);
    }
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
