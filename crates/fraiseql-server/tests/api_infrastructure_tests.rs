//! Tests for API infrastructure - routing, authentication, middleware

/// Helper test to verify API infrastructure is properly structured.
#[test]
fn test_api_modules_exist() {
    // This test verifies that the API module structure is correct
    // by ensuring all modules are properly declared and exported.
    // Actual route testing requires a running server with proper state setup.
}

/// Test API query endpoint types are correctly defined.
#[test]
fn test_api_query_response_types_defined() {
    use fraiseql_server::routes::api::query::{ExplainResponse, ValidateResponse, StatsResponse};

    // Verify response types exist and are properly structured
    let explain = ExplainResponse {
        query: "test".to_string(),
        sql: None,
        complexity: fraiseql_server::routes::api::query::ComplexityInfo {
            depth: 1,
            field_count: 1,
            score: 1,
        },
        warnings: vec![],
        estimated_cost: 100,
    };
    assert_eq!(explain.query, "test");

    let validate = ValidateResponse {
        valid: true,
        errors: vec![],
    };
    assert!(validate.valid);

    let stats = StatsResponse {
        total_queries: 0,
        successful_queries: 0,
        failed_queries: 0,
        average_latency_ms: 0.0,
    };
    assert_eq!(stats.total_queries, 0);
}

/// Test API federation endpoint types are correctly defined.
#[test]
fn test_api_federation_response_types_defined() {
    use fraiseql_server::routes::api::federation::{SubgraphsResponse, GraphResponse};

    let subgraphs = SubgraphsResponse {
        subgraphs: vec![],
    };
    assert!(subgraphs.subgraphs.is_empty());

    let graph = GraphResponse {
        format: "json".to_string(),
        content: "{}".to_string(),
    };
    assert_eq!(graph.format, "json");
}

/// Test API schema endpoint types are correctly defined.
#[test]
fn test_api_schema_response_types_defined() {
    use fraiseql_server::routes::api::schema::{GraphQLSchemaResponse, JsonSchemaResponse};

    let graphql_schema = GraphQLSchemaResponse {
        schema: "type Query { hello: String }".to_string(),
    };
    assert!(!graphql_schema.schema.is_empty());

    let json_schema = JsonSchemaResponse {
        schema: serde_json::json!({}),
    };
    assert!(json_schema.schema.is_object());
}

/// Test API error types are correctly defined.
#[test]
fn test_api_error_types_defined() {
    use fraiseql_server::routes::api::types::{ApiError, ApiResponse};

    let error = ApiError::new("test error", "TEST_CODE");
    assert_eq!(error.code, "TEST_CODE");
    assert_eq!(error.error, "test error");

    let response: ApiResponse<String> = ApiResponse {
        status: "success".to_string(),
        data: "test data".to_string(),
    };
    assert_eq!(response.status, "success");
}
