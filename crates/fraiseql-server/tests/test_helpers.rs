//! Test helpers for E2E testing.
//!
//! Provides utilities for:
//! - Starting test servers
//! - Creating test databases
//! - Managing test resources
//! - Common assertions

#![allow(dead_code)] // Test helper utilities may not be used in all test files
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// Test server configuration
pub struct TestServerConfig {
    /// Bind address
    pub bind_addr: String,
    /// Database URL
    pub database_url: String,
    /// Schema path
    pub schema_path: String,
}

impl TestServerConfig {
    /// Create with defaults
    pub fn new() -> Self {
        Self {
            bind_addr: "127.0.0.1:0".to_string(), // Random port
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgresql:///fraiseql_test".to_string()),
            schema_path: "schema.compiled.json".to_string(),
        }
    }

    /// Set database URL
    pub fn with_database_url(mut self, url: String) -> Self {
        self.database_url = url;
        self
    }

    /// Set schema path
    pub fn with_schema_path(mut self, path: String) -> Self {
        self.schema_path = path;
        self
    }
}

impl Default for TestServerConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Test server handle
pub struct TestServer {
    /// Server port
    pub port: u16,
    /// Base URL
    pub base_url: String,
    /// Server task handle
    _handle: Option<JoinHandle<()>>,
}

impl TestServer {
    /// Get GraphQL endpoint URL
    pub fn graphql_url(&self) -> String {
        format!("{}/graphql", self.base_url)
    }

    /// Get health endpoint URL
    pub fn health_url(&self) -> String {
        format!("{}/health", self.base_url)
    }

    /// Get metrics endpoint URL
    pub fn metrics_url(&self) -> String {
        format!("{}/metrics", self.base_url)
    }

    /// Get introspection endpoint URL
    pub fn introspection_url(&self) -> String {
        format!("{}/introspection", self.base_url)
    }
}

/// Find available port
pub async fn find_available_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to find available port");

    listener
        .local_addr()
        .expect("Failed to get local address")
        .port()
}

/// Get test HTTP client
pub fn create_test_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("Failed to create HTTP client")
}

/// Get metrics token from environment or use default E2E test token.
///
/// The token must match what's configured in docker-compose.e2e.yml.
pub fn get_metrics_token() -> String {
    std::env::var("FRAISEQL_METRICS_TOKEN")
        .unwrap_or_else(|_| "e2e-test-metrics-token-32chars!".to_string())
}

/// Create GraphQL request JSON
pub fn create_graphql_request(
    query: &str,
    variables: Option<serde_json::Value>,
    operation_name: Option<&str>,
) -> serde_json::Value {
    let mut request = serde_json::json!({
        "query": query
    });

    if let Some(vars) = variables {
        request["variables"] = vars;
    }

    if let Some(op) = operation_name {
        request["operationName"] = serde_json::json!(op);
    }

    request
}

/// Assert health check response
pub fn assert_health_response(response: &serde_json::Value) {
    assert!(response.get("status").is_some(), "Missing status field");
    assert!(
        response["status"].as_str() == Some("healthy") || response["status"].as_str() == Some("unhealthy"),
        "Invalid status value: expected 'healthy' or 'unhealthy', got {:?}",
        response["status"]
    );
}

/// Assert GraphQL response structure
pub fn assert_graphql_response(response: &serde_json::Value) {
    // Should have either data or errors
    let has_data = response.get("data").is_some();
    let has_errors = response.get("errors").is_some();
    assert!(has_data || has_errors, "GraphQL response missing data and errors");
}

/// Assert no errors in GraphQL response
pub fn assert_no_graphql_errors(response: &serde_json::Value) {
    if let Some(errors) = response.get("errors") {
        assert!(
            errors.as_array().map_or(true, |e| e.is_empty()),
            "Unexpected GraphQL errors: {}",
            errors
        );
    }
}

/// Assert metrics response structure
pub fn assert_metrics_response(response: &serde_json::Value) {
    assert!(response.get("queries_total").is_some(), "Missing queries_total");
    assert!(
        response.get("queries_success").is_some(),
        "Missing queries_success"
    );
    assert!(response.get("queries_error").is_some(), "Missing queries_error");
    assert!(
        response.get("avg_query_duration_ms").is_some(),
        "Missing avg_query_duration_ms"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = TestServerConfig::new();
        assert_eq!(config.bind_addr, "127.0.0.1:0");
        assert_eq!(config.schema_path, "schema.compiled.json");
    }

    #[test]
    fn test_config_builder() {
        let config = TestServerConfig::new()
            .with_database_url("postgresql:///custom_db".to_string())
            .with_schema_path("custom_schema.json".to_string());

        assert_eq!(config.database_url, "postgresql:///custom_db");
        assert_eq!(config.schema_path, "custom_schema.json");
    }

    #[tokio::test]
    async fn test_find_available_port() {
        let port = find_available_port().await;
        assert!(port > 0);
    }

    #[test]
    fn test_create_test_client() {
        let client = create_test_client();
        // Just verify it creates without error
        assert!(client.get("http://localhost:8080").build().is_ok());
    }

    #[test]
    fn test_create_graphql_request_minimal() {
        let request = create_graphql_request("{ user { id } }", None, None);
        assert_eq!(request["query"], "{ user { id } }");
        assert!(request.get("variables").is_none());
        assert!(request.get("operationName").is_none());
    }

    #[test]
    fn test_create_graphql_request_with_variables() {
        let vars = serde_json::json!({"id": "123"});
        let request = create_graphql_request("query($id: ID!) { user(id: $id) { id } }", Some(vars.clone()), None);

        assert_eq!(request["variables"], vars);
    }

    #[test]
    fn test_create_graphql_request_with_operation() {
        let request = create_graphql_request("query GetUser { user { id } }", None, Some("GetUser"));
        assert_eq!(request["operationName"], "GetUser");
    }

    #[test]
    fn test_assert_health_response() {
        let response = serde_json::json!({
            "status": "healthy",
            "database": {
                "connected": true
            }
        });
        assert_health_response(&response); // Should not panic
    }

    #[test]
    fn test_assert_graphql_response_with_data() {
        let response = serde_json::json!({
            "data": {
                "user": {"id": "123"}
            }
        });
        assert_graphql_response(&response); // Should not panic
    }

    #[test]
    fn test_assert_graphql_response_with_errors() {
        let response = serde_json::json!({
            "errors": [
                {"message": "Invalid query"}
            ]
        });
        assert_graphql_response(&response); // Should not panic
    }

    #[test]
    fn test_assert_no_graphql_errors() {
        let response = serde_json::json!({
            "data": {"user": {"id": "123"}},
            "errors": []
        });
        assert_no_graphql_errors(&response); // Should not panic
    }

    #[test]
    #[should_panic]
    fn test_assert_no_graphql_errors_panics() {
        let response = serde_json::json!({
            "errors": [
                {"message": "Error"}
            ]
        });
        assert_no_graphql_errors(&response);
    }

    #[test]
    fn test_assert_metrics_response() {
        let response = serde_json::json!({
            "queries_total": 100,
            "queries_success": 95,
            "queries_error": 5,
            "avg_query_duration_ms": 25.5,
            "cache_hit_ratio": 0.75
        });
        assert_metrics_response(&response); // Should not panic
    }
}
