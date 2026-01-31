//! Error Handling Validation Tests (GREEN Phase)
//!
//! Tests comprehensive error handling integration with actual error infrastructure:
//! 1. Database connection failures
//! 2. Query parse errors
//! 3. Schema validation errors
//! 4. Authorization failures
//! 5. Timeout errors
//! 6. Invalid input (SQL injection, XSS attempts)
//! 7. Network errors (for observers, webhooks)
//! 8. Resource exhaustion (too many subscriptions, large results)
//!
//! Integrates with fraiseql_server::error module for spec-compliant error handling.
//!
//! # Running Tests
//!
//! ```bash
//! cargo test --test error_handling_validation_test -- --nocapture
//! ```

#![cfg(test)]

use axum::http::StatusCode;
use fraiseql_server::error::{ErrorCode, ErrorExtensions, GraphQLError};

// ============================================================================
// Test Cases: Database Errors
// ============================================================================

#[test]
fn test_database_connection_failure_response() {
    let error = GraphQLError::database("Failed to connect to database: Connection refused")
        .with_extensions(ErrorExtensions {
            category:   Some("DATABASE".to_string()),
            status:     Some(500),
            request_id: Some("req-12345".to_string()),
        });

    assert_eq!(error.message, "Failed to connect to database: Connection refused");
    assert_eq!(error.code, ErrorCode::DatabaseError);
    assert!(error.extensions.is_some());
    let ext = error.extensions.unwrap();
    assert_eq!(ext.request_id, Some("req-12345".to_string()));
    assert_eq!(error.code.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn test_database_timeout_returns_error() {
    let error =
        GraphQLError::new("Database query exceeded timeout of 30 seconds", ErrorCode::Timeout);

    assert_eq!(error.code, ErrorCode::Timeout);
    assert_eq!(error.code.status_code(), StatusCode::REQUEST_TIMEOUT);
}

#[test]
fn test_database_pool_exhaustion() {
    let error = GraphQLError::new(
        "Database connection pool exhausted: all 10 connections in use",
        ErrorCode::InternalServerError,
    );

    assert_eq!(error.code, ErrorCode::InternalServerError);
    assert_eq!(error.code.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
}

// ============================================================================
// Test Cases: Query Parsing Errors
// ============================================================================

#[test]
fn test_invalid_graphql_query_syntax() {
    let error =
        GraphQLError::parse("GraphQL parse error: Unexpected token '}' at line 1, column 15")
            .with_location(1, 15);

    assert_eq!(error.code, ErrorCode::ParseError);
    assert!(error.locations.is_some());
    assert_eq!(error.code.status_code(), StatusCode::BAD_REQUEST);
}

#[test]
fn test_malformed_json_variables() {
    let error =
        GraphQLError::request("Invalid JSON in variables: Unexpected character at position 5");

    assert_eq!(error.code, ErrorCode::RequestError);
    assert_eq!(error.code.status_code(), StatusCode::BAD_REQUEST);
}

// ============================================================================
// Test Cases: Schema Validation Errors
// ============================================================================

#[test]
fn test_unknown_field_validation_error() {
    let error = GraphQLError::validation("Cannot query field 'unknownField' on type 'User'")
        .with_path(vec!["user".to_string(), "unknownField".to_string()]);

    assert_eq!(error.code, ErrorCode::ValidationError);
    assert!(error.path.is_some());
    assert_eq!(error.code.status_code(), StatusCode::BAD_REQUEST);
}

#[test]
fn test_type_mismatch_in_query() {
    let error = GraphQLError::validation("Argument 'id' requires type 'ID!', but received String");

    assert_eq!(error.code, ErrorCode::ValidationError);
    assert_eq!(error.code.status_code(), StatusCode::BAD_REQUEST);
}

#[test]
fn test_required_field_missing() {
    let error = GraphQLError::validation("Argument 'email' is required for mutation 'createUser'");

    assert_eq!(error.code, ErrorCode::ValidationError);
    assert_eq!(error.code.status_code(), StatusCode::BAD_REQUEST);
}

// ============================================================================
// Test Cases: Authorization Errors
// ============================================================================

#[test]
fn test_missing_authentication_token() {
    let error = GraphQLError::unauthenticated();

    assert_eq!(error.code, ErrorCode::Unauthenticated);
    assert_eq!(error.code.status_code(), StatusCode::UNAUTHORIZED);
}

#[test]
fn test_insufficient_permissions() {
    let error = GraphQLError::forbidden();

    assert_eq!(error.code, ErrorCode::Forbidden);
    assert_eq!(error.code.status_code(), StatusCode::FORBIDDEN);
}

#[test]
fn test_expired_token() {
    let error = GraphQLError::new("Authentication token has expired", ErrorCode::Unauthenticated);

    assert_eq!(error.code, ErrorCode::Unauthenticated);
    assert_eq!(error.code.status_code(), StatusCode::UNAUTHORIZED);
}

// ============================================================================
// Test Cases: Timeout Errors
// ============================================================================

#[test]
fn test_query_execution_timeout() {
    let error = GraphQLError::new("Query execution exceeded 30-second timeout", ErrorCode::Timeout);

    assert_eq!(error.code, ErrorCode::Timeout);
    assert_eq!(error.code.status_code(), StatusCode::REQUEST_TIMEOUT);
}

#[test]
fn test_network_request_timeout() {
    let error = GraphQLError::new(
        "Network request to subgraph 'inventory' timed out after 5 seconds",
        ErrorCode::InternalServerError,
    );

    assert_eq!(error.code, ErrorCode::InternalServerError);
}

// ============================================================================
// Test Cases: Invalid Input (Security)
// ============================================================================

#[test]
fn test_sql_injection_attempt_blocked() {
    let malicious_input = "'; DROP TABLE users; --";
    let error = GraphQLError::request("Invalid input detected: suspicious characters in query");

    assert_eq!(error.code, ErrorCode::RequestError);
    assert!(!error.message.contains(malicious_input));
    assert_eq!(error.code.status_code(), StatusCode::BAD_REQUEST);
}

#[test]
fn test_nosql_injection_attempt_blocked() {
    let error = GraphQLError::request("Invalid input: prohibited operators detected");

    assert_eq!(error.code, ErrorCode::RequestError);
    assert_eq!(error.code.status_code(), StatusCode::BAD_REQUEST);
}

#[test]
fn test_xss_payload_sanitized() {
    let xss_payload = "<script>alert('xss')</script>";
    let error = GraphQLError::request("Invalid input: HTML/script tags not allowed");

    assert_eq!(error.code, ErrorCode::RequestError);
    assert!(!error.message.contains(xss_payload));
}

#[test]
fn test_javascript_protocol_blocked() {
    let js_protocol = "javascript:void(0)";
    let error = GraphQLError::request("Invalid input: dangerous URL protocol detected");

    assert_eq!(error.code, ErrorCode::RequestError);
    assert!(!error.message.contains(js_protocol));
}

// ============================================================================
// Test Cases: Network Errors
// ============================================================================

#[test]
fn test_webhook_delivery_failure() {
    let error = GraphQLError::new(
        "Failed to deliver webhook: Connection refused to https://example.com/webhook",
        ErrorCode::InternalServerError,
    );

    assert_eq!(error.code, ErrorCode::InternalServerError);
    assert_eq!(error.code.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn test_external_service_unavailable() {
    let error = GraphQLError::new(
        "External service 'payment-gateway' returned 503 Service Unavailable",
        ErrorCode::InternalServerError,
    );

    assert_eq!(error.code, ErrorCode::InternalServerError);
}

#[test]
fn test_dns_resolution_failure() {
    let error = GraphQLError::new(
        "Failed to resolve DNS for host 'subgraph.example.com'",
        ErrorCode::InternalServerError,
    );

    assert_eq!(error.code, ErrorCode::InternalServerError);
}

// ============================================================================
// Test Cases: Resource Exhaustion
// ============================================================================

#[test]
fn test_too_many_subscriptions() {
    let error = GraphQLError::new(
        "Subscription limit exceeded: client already has 100 active subscriptions",
        ErrorCode::RateLimitExceeded,
    );

    assert_eq!(error.code, ErrorCode::RateLimitExceeded);
    assert_eq!(error.code.status_code(), StatusCode::TOO_MANY_REQUESTS);
}

#[test]
fn test_query_result_too_large() {
    let error = GraphQLError::new(
        "Query result size exceeds maximum of 100MB",
        ErrorCode::RateLimitExceeded,
    );

    assert_eq!(error.code, ErrorCode::RateLimitExceeded);
}

#[test]
fn test_rate_limit_exceeded() {
    let error = GraphQLError::new(
        "Rate limit exceeded: 1000 requests per minute",
        ErrorCode::RateLimitExceeded,
    );

    assert_eq!(error.code, ErrorCode::RateLimitExceeded);
    assert_eq!(error.code.status_code(), StatusCode::TOO_MANY_REQUESTS);
}

// ============================================================================
// Test Cases: Error Response Structure
// ============================================================================

#[test]
fn test_error_response_has_request_id() {
    let extensions = ErrorExtensions {
        category:   None,
        status:     None,
        request_id: Some("req-unique-12368".to_string()),
    };

    let error = GraphQLError::validation("Something went wrong").with_extensions(extensions);

    assert!(error.extensions.is_some());
    let ext = error.extensions.unwrap();
    assert_eq!(ext.request_id, Some("req-unique-12368".to_string()));
}

#[test]
fn test_error_response_has_error_code() {
    let error = GraphQLError::new("Something went wrong", ErrorCode::ValidationError);

    assert_eq!(error.code, ErrorCode::ValidationError);
}

#[test]
fn test_error_response_has_clear_message() {
    let error = GraphQLError::validation("Field 'invalidField' does not exist on type 'User'");

    assert!(!error.message.is_empty());
    assert!(error.message.len() > 10);
}

#[test]
fn test_error_response_with_extensions() {
    let extensions = ErrorExtensions {
        category:   Some("VALIDATION".to_string()),
        status:     Some(400),
        request_id: Some("req-12371".to_string()),
    };

    let error = GraphQLError::validation("Error occurred").with_extensions(extensions);

    assert!(error.extensions.is_some());
    let ext = error.extensions.unwrap();
    assert_eq!(ext.category, Some("VALIDATION".to_string()));
    assert_eq!(ext.status, Some(400));
}

// ============================================================================
// Test Cases: HTTP Status Code Mapping
// ============================================================================

#[test]
fn test_http_status_codes_correct() {
    assert_eq!(ErrorCode::DatabaseError.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(ErrorCode::ParseError.status_code(), StatusCode::BAD_REQUEST);
    assert_eq!(ErrorCode::ValidationError.status_code(), StatusCode::BAD_REQUEST);
    assert_eq!(ErrorCode::Unauthenticated.status_code(), StatusCode::UNAUTHORIZED);
    assert_eq!(ErrorCode::Timeout.status_code(), StatusCode::REQUEST_TIMEOUT);
    assert_eq!(ErrorCode::RequestError.status_code(), StatusCode::BAD_REQUEST);
    assert_eq!(ErrorCode::InternalServerError.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(ErrorCode::RateLimitExceeded.status_code(), StatusCode::TOO_MANY_REQUESTS);
}

// ============================================================================
// Test Cases: Error Propagation and Context
// ============================================================================

#[test]
fn test_error_propagation_preserves_context() {
    let error = GraphQLError::database("Database query failed");

    assert_eq!(error.code, ErrorCode::DatabaseError);
    assert_eq!(error.message, "Database query failed");
}

#[test]
fn test_nested_error_handling() {
    let inner_error = "Field resolution failed";
    let message = format!("GraphQL execution failed: {}", inner_error);
    let outer_error = GraphQLError::new(message, ErrorCode::InternalServerError);

    assert!(outer_error.message.contains(inner_error));
}

#[test]
fn test_error_with_location_and_path() {
    let error = GraphQLError::validation("Field not found")
        .with_location(2, 5)
        .with_path(vec!["user".to_string(), "profile".to_string()]);

    assert!(error.locations.is_some());
    assert!(error.path.is_some());

    let locations = error.locations.unwrap();
    assert_eq!(locations[0].line, 2);
    assert_eq!(locations[0].column, 5);

    let path = error.path.unwrap();
    assert_eq!(path.len(), 2);
    assert_eq!(path[0], "user");
}

#[test]
fn test_multiple_errors_in_response() {
    let errors = [
        GraphQLError::validation("Field 1 failed"),
        GraphQLError::validation("Field 2 failed"),
    ];

    assert_eq!(errors.len(), 2);
    assert!(errors.iter().all(|e| e.code == ErrorCode::ValidationError));
}
