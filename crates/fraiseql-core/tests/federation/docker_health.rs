//! Docker Compose Federation Integration Tests - Service Health & Composition
//!
//! Tests validate service startup, health checks, and schema composition.

use super::common::*;

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
