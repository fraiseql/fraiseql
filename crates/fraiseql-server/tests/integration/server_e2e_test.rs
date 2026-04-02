//! End-to-end integration tests for FraiseQL HTTP server.
//!
//! Tests the complete HTTP server functionality including:
//! - GraphQL query execution through HTTP endpoints
//! - Error handling and validation
//! - Response formatting
//! - Concurrent request handling
//!
//! **Execution engine:** none
//! **Infrastructure:** none
//! **Parallelism:** safe

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use fraiseql_server::{
    error::{ErrorCode, ErrorExtensions, ErrorResponse, GraphQLError},
    routes::graphql::GraphQLRequest,
    validation::RequestValidator,
};
use serde_json::json;

/// Test that validation catches empty queries
#[test]
fn test_empty_query_validation() {
    let validator = RequestValidator::new();

    let result = validator.validate_query("");
    assert!(result.is_err(), "expected Err for empty query, got: {result:?}");

    let result = validator.validate_query("   ");
    assert!(result.is_err(), "expected Err for whitespace-only query, got: {result:?}");
}

/// Test that depth validation correctly identifies deeply nested queries
#[test]
fn test_depth_validation() {
    let validator = RequestValidator::new().with_max_depth(3);

    // Shallow query should pass
    let shallow = "{ user { id } }";
    validator
        .validate_query(shallow)
        .unwrap_or_else(|e| panic!("expected Ok for shallow query: {e}"));

    // Medium query should pass
    let medium = "{ user { profile { settings } } }";
    validator
        .validate_query(medium)
        .unwrap_or_else(|e| panic!("expected Ok for medium query: {e}"));

    // Deep query should fail
    let deep = "{ user { profile { settings { theme { dark } } } } }";
    assert!(validator.validate_query(deep).is_err(), "expected Err for deep query, got Ok");
}

/// Test that complexity validation prevents overly complex queries
#[test]
fn test_complexity_validation() {
    let validator = RequestValidator::new().with_max_complexity(5);

    // Simple query should pass
    let simple = "{ user { id name } }";
    validator
        .validate_query(simple)
        .unwrap_or_else(|e| panic!("expected Ok for simple query: {e}"));

    // Complex query with many brackets should fail
    let complex = "{ users [ posts [ comments [ author [ name ] ] ] ] }";
    assert!(
        validator.validate_query(complex).is_err(),
        "expected Err for complex query, got Ok"
    );
}

/// Test that variables validation works correctly
#[test]
fn test_variables_validation() {
    let validator = RequestValidator::new();

    // Valid variables object
    let valid = json!({
        "id": "123",
        "name": "John"
    });
    validator
        .validate_variables(Some(&valid))
        .unwrap_or_else(|e| panic!("expected Ok for valid variables: {e}"));

    // No variables is valid
    validator
        .validate_variables(None)
        .unwrap_or_else(|e| panic!("expected Ok for None variables: {e}"));

    // Variables as array is invalid
    let invalid = json!([1, 2, 3]);
    assert!(
        validator.validate_variables(Some(&invalid)).is_err(),
        "expected Err for array variables, got Ok"
    );
}

/// Test that validation can be disabled
#[test]
fn test_disable_validation() {
    let validator = RequestValidator::new()
        .with_depth_validation(false)
        .with_complexity_validation(false)
        .with_max_depth(1)
        .with_max_complexity(1);

    // Very deep and complex query should pass when validation disabled
    let deep = "{ a { b { c { d { e { f } } } } } }";
    validator
        .validate_query(deep)
        .unwrap_or_else(|e| panic!("expected Ok when validation disabled: {e}"));
}

/// Test `GraphQLError` serialization
#[test]
fn test_error_serialization() {
    let error = GraphQLError::validation("Invalid query")
        .with_location(1, 5)
        .with_path(vec!["user".to_string(), "id".to_string()]);

    let json: serde_json::Value = serde_json::to_value(&error).unwrap();
    assert_eq!(json["message"], "Invalid query");
    assert_eq!(json["code"], "VALIDATION_ERROR");
    assert_eq!(json["locations"][0]["line"], 1);
    assert_eq!(json["locations"][0]["column"], 5);
    let path = json["path"].as_array().unwrap();
    assert_eq!(path.len(), 2);
    assert_eq!(path[0], "user");
    assert_eq!(path[1], "id");
}

/// Test different error code HTTP status mappings
#[test]
fn test_error_code_status_mapping() {
    assert_eq!(ErrorCode::ValidationError.status_code(), axum::http::StatusCode::OK);
    assert_eq!(ErrorCode::Unauthenticated.status_code(), axum::http::StatusCode::UNAUTHORIZED);
    assert_eq!(ErrorCode::Forbidden.status_code(), axum::http::StatusCode::FORBIDDEN);
    assert_eq!(ErrorCode::NotFound.status_code(), axum::http::StatusCode::NOT_FOUND);
    assert_eq!(
        ErrorCode::DatabaseError.status_code(),
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        ErrorCode::RateLimitExceeded.status_code(),
        axum::http::StatusCode::TOO_MANY_REQUESTS
    );
}

/// Test GraphQL request deserialization
#[test]
fn test_graphql_request_deserialization() {
    let json_str = r#"{"query": "{ users { id } }"}"#;
    let request: GraphQLRequest = serde_json::from_str(json_str).unwrap();

    assert_eq!(request.query.as_deref(), Some("{ users { id } }"));
    assert_eq!(request.variables, None);
    assert_eq!(request.operation_name, None);
}

/// Test GraphQL request with variables deserialization
#[test]
fn test_graphql_request_with_variables_deserialization() {
    let json_str =
        r#"{"query": "query($id: ID!) { user(id: $id) { name } }", "variables": {"id": "123"}}"#;
    let request: GraphQLRequest = serde_json::from_str(json_str).unwrap();

    assert_eq!(request.query.as_deref(), Some("query($id: ID!) { user(id: $id) { name } }"));
    let variables = request.variables.expect("variables should be present");
    assert_eq!(variables, json!({"id": "123"}));
}

/// Test GraphQL request with operation name
#[test]
fn test_graphql_request_with_operation_name() {
    let json = r#"{
        "query": "query GetUser { user { id } }",
        "operationName": "GetUser"
    }"#;
    let request: GraphQLRequest = serde_json::from_str(json).unwrap();

    assert_eq!(request.operation_name, Some("GetUser".to_string()));
}

/// Test request validation integration
#[test]
fn test_request_validation_integration() {
    let validator = RequestValidator::new();

    // Test with valid request
    let valid_request = GraphQLRequest {
        query:          Some("{ user { id } }".to_string()),
        variables:      None,
        operation_name: None,
        extensions:     None,
        document_id:    None,
    };

    validator
        .validate_query(valid_request.query.as_deref().unwrap())
        .unwrap_or_else(|e| panic!("expected Ok for valid request query: {e}"));
    validator
        .validate_variables(valid_request.variables.as_ref())
        .unwrap_or_else(|e| panic!("expected Ok for valid request variables: {e}"));

    // Test with invalid depth
    let deep_request = GraphQLRequest {
        query:          Some("{ a { b { c { d { e { f } } } } } }".to_string()),
        variables:      None,
        operation_name: None,
        extensions:     None,
        document_id:    None,
    };

    let validator = validator.with_max_depth(2);
    assert!(
        validator.validate_query(deep_request.query.as_deref().unwrap()).is_err(),
        "expected Err for query exceeding max depth 2, got Ok"
    );
}

/// Test error response with multiple errors
#[test]
fn test_multiple_errors_response() {
    let response = ErrorResponse::new(vec![
        GraphQLError::validation("Field not found"),
        GraphQLError::database("Connection timeout"),
    ]);

    let json: serde_json::Value = serde_json::to_value(&response).unwrap();
    let errors = json["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 2);
    assert_eq!(errors[0]["message"], "Field not found");
    assert_eq!(errors[0]["code"], "VALIDATION_ERROR");
    assert_eq!(errors[1]["message"], "Connection timeout");
    assert_eq!(errors[1]["code"], "DATABASE_ERROR");
}

/// Test error extensions functionality
#[test]
fn test_error_extensions() {
    let extensions = ErrorExtensions {
        category:         Some("VALIDATION".to_string()),
        status:           Some(400),
        request_id:       Some("req-12345".to_string()),
        retry_after_secs: None,
        detail:           None,
    };

    let error = GraphQLError::validation("Invalid input").with_extensions(extensions);
    let json: serde_json::Value = serde_json::to_value(&error).unwrap();

    assert_eq!(json["message"], "Invalid input");
    assert_eq!(json["code"], "VALIDATION_ERROR");
    assert_eq!(json["extensions"]["category"], "VALIDATION");
    assert_eq!(json["extensions"]["status"], 400);
    assert_eq!(json["extensions"]["request_id"], "req-12345");
}

/// Test validator builder pattern
#[test]
fn test_validator_builder_pattern() {
    let validator = RequestValidator::new()
        .with_max_depth(5)
        .with_max_complexity(50)
        .with_depth_validation(true)
        .with_complexity_validation(false);

    // Verify settings are applied through builder chain
    let deep = "{ a { b { c { d { e { f } } } } } }";
    assert!(
        validator.validate_query(deep).is_err(),
        "expected Err for deep query exceeding depth limit"
    ); // Depth check still works

    // AST parser correctly rejects invalid syntax (square brackets) regardless
    // of complexity validation setting
    let invalid_syntax = "{ a [ b [ c [ d [ e ] ] ] ] }";
    assert!(
        validator.validate_query(invalid_syntax).is_err(),
        "expected Err for invalid syntax, got Ok"
    );

    // A valid complex query should pass when complexity validation is disabled
    let complex = "{ a { b { c { d { e } } } } }";
    validator
        .validate_query(complex)
        .unwrap_or_else(|e| panic!("expected Ok when complexity validation disabled: {e}"));
}

/// Test GraphQL error code to HTTP status mapping completeness
#[test]
fn test_all_error_codes_have_status() {
    // This test ensures all error code variants have a status code mapping
    let codes = vec![
        ErrorCode::ValidationError,
        ErrorCode::ParseError,
        ErrorCode::RequestError,
        ErrorCode::Unauthenticated,
        ErrorCode::Forbidden,
        ErrorCode::NotFound,
        ErrorCode::Conflict,
        ErrorCode::DatabaseError,
        ErrorCode::InternalServerError,
        ErrorCode::Timeout,
        ErrorCode::RateLimitExceeded,
        ErrorCode::CircuitBreakerOpen,
        ErrorCode::PersistedQueryNotFound,
        ErrorCode::PersistedQueryMismatch,
        ErrorCode::ForbiddenQuery,
        ErrorCode::DocumentNotFound,
    ];

    // Variants that correctly return 200 per GraphQL-over-HTTP spec §7.1.2
    let ok_variants = [
        ErrorCode::ValidationError,
        ErrorCode::ParseError,
        ErrorCode::PersistedQueryNotFound,
    ];

    for code in codes {
        let status = code.status_code();
        if ok_variants.contains(&code) {
            assert_eq!(status, axum::http::StatusCode::OK, "{code:?} should return 200");
        } else {
            assert!(
                status.is_client_error() || status.is_server_error(),
                "{code:?} should return 4xx/5xx"
            );
        }
    }
}

/// Test that error responses can be converted to HTTP responses with correct status
#[test]
fn test_error_response_into_response() {
    use axum::response::IntoResponse;

    let error = GraphQLError::validation("Test error");
    let response = ErrorResponse::from_error(error);

    let http_response = response.into_response();
    assert_eq!(http_response.status(), axum::http::StatusCode::OK);
}

/// Test string handling in query validation
#[test]
fn test_string_literal_handling() {
    let validator = RequestValidator::new();

    // String literals in selection sets are not valid GraphQL syntax -
    // the AST parser correctly rejects them
    let query = r#"{ user { name: "John \"Doe\"" } }"#;
    let result = validator.validate_query(query);
    assert!(result.is_err(), "String literals in selection sets are invalid GraphQL syntax");

    // Valid: query with string argument
    let valid_query = r#"query { user(name: "John") { id name } }"#;
    validator
        .validate_query(valid_query)
        .unwrap_or_else(|e| panic!("String arguments in field invocations should be valid: {e}"));
}

/// Test validator with minimal configuration
#[test]
fn test_minimal_validator() {
    let validator = RequestValidator::new();

    // Default validator should accept basic queries
    let simple = "{ user }";
    validator
        .validate_query(simple)
        .unwrap_or_else(|e| panic!("expected Ok for simple query: {e}"));
}

/// Test error from validation error variant
#[test]
fn test_validation_error_conversion() {
    let error = fraiseql_server::ComplexityValidationError::QueryTooDeep {
        max_depth:    10,
        actual_depth: 15,
    };

    let error_msg = error.to_string();
    assert_eq!(
        error_msg, "Query exceeds maximum depth of 10: depth = 15",
        "ValidationError::QueryTooDeep should produce exact error message"
    );
}

/// Test various `GraphQLError` factory methods
#[test]
fn test_graphql_error_factory_methods() {
    let validation_error = GraphQLError::validation("Validation failed");
    assert_eq!(validation_error.code, ErrorCode::ValidationError);

    let parse_error = GraphQLError::parse("Parse failed");
    assert_eq!(parse_error.code, ErrorCode::ParseError);

    let request_error = GraphQLError::request("Request failed");
    assert_eq!(request_error.code, ErrorCode::RequestError);

    let db_error = GraphQLError::database("DB failed");
    assert_eq!(db_error.code, ErrorCode::DatabaseError);

    let internal_error = GraphQLError::internal("Internal error");
    assert_eq!(internal_error.code, ErrorCode::InternalServerError);

    let execution_error = GraphQLError::execution("Execution failed");
    assert_eq!(execution_error.code, ErrorCode::InternalServerError);

    let not_found_error = GraphQLError::not_found("Not found");
    assert_eq!(not_found_error.code, ErrorCode::NotFound);

    let unauthenticated = GraphQLError::unauthenticated();
    assert_eq!(unauthenticated.code, ErrorCode::Unauthenticated);

    let forbidden = GraphQLError::forbidden();
    assert_eq!(forbidden.code, ErrorCode::Forbidden);
}
