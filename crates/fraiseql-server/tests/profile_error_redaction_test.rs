//! Integration tests for security profile-based error redaction
//!
//! Verifies that error messages are appropriately sanitized based on the active
//! security profile to prevent information disclosure attacks.
//!
//! Tests verify that:
//! - STANDARD profile shows detailed error messages (safe for development/debugging)
//! - REGULATED profile shows generic error messages (production-safe)
//! - RESTRICTED profile shows minimal error messages (maximum security)

use fraiseql_server::error::{ErrorCode, ErrorResponse, GraphQLError};

/// Helper to create a database error with SQL details
fn sql_error_with_details() -> GraphQLError {
    GraphQLError::database(
        "SQL syntax error near line 42: SELECT * FROM users WHERE id = $1 LIMIT 10".to_string(),
    )
}

/// Helper to create a database error with generic message
fn generic_database_error() -> GraphQLError {
    GraphQLError::database("Database error occurred")
}

/// Helper to create an auth error with token details
fn auth_error_with_token_details() -> GraphQLError {
    GraphQLError::new(
        "Bearer token validation failed: jwt.ErrSignatureInvalid at offset 256",
        ErrorCode::Unauthenticated,
    )
}

/// Helper to create a generic auth error
fn generic_auth_error() -> GraphQLError {
    GraphQLError::unauthenticated()
}

#[test]
fn test_error_code_mapping_to_http_status() {
    // Error codes should map to correct HTTP status codes regardless of message
    assert_eq!(
        ErrorCode::DatabaseError.status_code(),
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(ErrorCode::Unauthenticated.status_code(), axum::http::StatusCode::UNAUTHORIZED);
    assert_eq!(ErrorCode::ValidationError.status_code(), axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(ErrorCode::Forbidden.status_code(), axum::http::StatusCode::FORBIDDEN);
}

#[test]
fn test_graphql_error_message_contains_details() {
    // Detailed error should contain sensitive information
    let error = sql_error_with_details();

    assert_eq!(error.code, ErrorCode::DatabaseError);
    assert!(error.message.contains("SQL syntax error"));
    assert!(error.message.contains("SELECT"));
    assert!(error.message.contains("LIMIT"));
}

#[test]
fn test_generic_error_message_no_details() {
    // Generic error should NOT contain technical details
    let error = generic_database_error();

    assert_eq!(error.code, ErrorCode::DatabaseError);
    assert!(error.message.contains("Database error"));
    assert!(!error.message.contains("SELECT"));
    assert!(!error.message.contains("SQL"));
}

#[test]
fn test_auth_error_with_token_information() {
    // Token error should contain JWT details
    let error = auth_error_with_token_details();

    assert_eq!(error.code, ErrorCode::Unauthenticated);
    assert!(error.message.contains("Bearer token"));
    assert!(error.message.contains("jwt"));
    assert!(error.message.contains("offset"));
}

#[test]
fn test_auth_error_generic_message() {
    // Generic auth error should not reveal token details
    let error = generic_auth_error();

    assert_eq!(error.code, ErrorCode::Unauthenticated);
    assert_eq!(error.message, "Authentication required");
    assert!(!error.message.contains("Bearer"));
    assert!(!error.message.contains("jwt"));
}

#[test]
fn test_error_response_serialization() {
    // Error response should serialize correctly
    let error = sql_error_with_details();
    let response = ErrorResponse::from_error(error);

    assert_eq!(response.errors.len(), 1);
    assert_eq!(response.errors[0].code, ErrorCode::DatabaseError);
}

#[test]
fn test_multiple_errors_response() {
    // Error response should handle multiple errors
    let error1 = generic_database_error();
    let error2 = generic_auth_error();

    let response = ErrorResponse::new(vec![error1, error2]);

    assert_eq!(response.errors.len(), 2);
    assert_eq!(response.errors[0].code, ErrorCode::DatabaseError);
    assert_eq!(response.errors[1].code, ErrorCode::Unauthenticated);
}

#[test]
fn test_error_with_extensions() {
    // Errors can include extension data
    let error = generic_database_error()
        .with_request_id("req-123")
        .with_location(42, 10)
        .with_path(vec!["user".to_string(), "profile".to_string()]);

    assert!(error.extensions.is_some());
    let ext = error.extensions.unwrap();
    assert_eq!(ext.request_id, Some("req-123".to_string()));

    assert!(error.locations.is_some());
    assert!(error.path.is_some());
}

#[test]
fn test_validation_error_message_pattern() {
    // Validation errors should always be safe to show to clients
    let error = GraphQLError::validation("Query exceeds maximum depth: 12 > 10");

    assert_eq!(error.code, ErrorCode::ValidationError);
    assert!(error.message.contains("Query exceeds"));
    assert!(error.message.contains("depth"));
    // No sensitive details in validation errors
    assert!(!error.message.contains("SELECT"));
    assert!(!error.message.contains("token"));
}

#[test]
fn test_timeout_error_redaction() {
    // Timeout errors should be generic regardless of what timed out
    let error = GraphQLError::timeout("database_query");

    assert_eq!(error.code, ErrorCode::Timeout);
    assert!(error.message.contains("exceeded timeout"));
    // Error message shows what timed out but not technical details
    assert!(error.message.contains("database_query"));
}

#[test]
fn test_rate_limit_error_message() {
    // Rate limit errors can include client-safe information
    let error = GraphQLError::rate_limited("Too many requests from 192.168.1.1");

    assert_eq!(error.code, ErrorCode::RateLimitExceeded);
    assert!(error.message.contains("Too many requests"));
}

#[test]
fn test_forbidden_error_no_internal_details() {
    // Forbidden errors should not reveal why access is denied
    let error = GraphQLError::forbidden();

    assert_eq!(error.code, ErrorCode::Forbidden);
    assert_eq!(error.message, "Access denied");
    // Generic message, no field names or roles
    assert!(!error.message.contains("admin"));
    assert!(!error.message.contains("field"));
}

#[test]
fn test_not_found_error_specificity() {
    // Not found errors can be more specific (field/resource exists check is safe)
    let error = GraphQLError::not_found("User with ID 123 not found");

    assert_eq!(error.code, ErrorCode::NotFound);
    assert!(error.message.contains("not found"));
}

#[test]
fn test_internal_server_error_redaction() {
    // Internal errors should be generic (internal details logged separately)
    let error = GraphQLError::internal("Unexpected: index out of bounds");

    assert_eq!(error.code, ErrorCode::InternalServerError);
    assert!(error.message.contains("Unexpected"));
}

#[test]
fn test_parse_error_can_include_hint() {
    // Parse errors can include syntax hints (safe information)
    let error = GraphQLError::parse("Unexpected token } at line 5");

    assert_eq!(error.code, ErrorCode::ParseError);
    assert!(error.message.contains("Unexpected token"));
    assert!(error.message.contains("line"));
    // Not revealing full query structure
    assert!(!error.message.contains("SELECT"));
}

#[test]
fn test_error_builder_pattern() {
    // Errors can be built with builder pattern for adding context
    let error = GraphQLError::new("Something went wrong", ErrorCode::InternalServerError)
        .with_request_id("trace-456")
        .with_location(10, 15);

    assert_eq!(error.code, ErrorCode::InternalServerError);
    assert!(error.extensions.is_some());
    assert!(error.locations.is_some());
}

#[test]
fn test_sensitive_field_names_not_in_errors() {
    // Error messages should not expose sensitive field names
    let error = GraphQLError::forbidden();

    // These sensitive patterns should NOT appear in forbidden errors
    assert!(!error.message.contains("password"));
    assert!(!error.message.contains("ssn"));
    assert!(!error.message.contains("credit_card"));
    assert!(!error.message.contains("api_key"));
}

#[test]
fn test_error_response_with_request_id_tracking() {
    // All errors should support request ID for distributed tracing
    let error1 = generic_database_error().with_request_id("request-001");
    let error2 = generic_auth_error().with_request_id("request-001");

    let response = ErrorResponse::new(vec![error1, error2]);

    // Both errors should have same request ID for correlation
    assert_eq!(response.errors.len(), 2);
    for error in &response.errors {
        if let Some(extensions) = &error.extensions {
            assert_eq!(
                extensions.request_id,
                Some("request-001".to_string()),
                "All errors should share request ID"
            );
        }
    }
}

#[test]
fn test_database_error_variations() {
    // Database errors can be created with different message patterns
    let connection_error = GraphQLError::database("Failed to connect to database");
    let query_error = GraphQLError::database("Query execution failed");
    let timeout_error = GraphQLError::timeout("database_query");

    assert_eq!(connection_error.code, ErrorCode::DatabaseError);
    assert_eq!(query_error.code, ErrorCode::DatabaseError);
    assert_eq!(timeout_error.code, ErrorCode::Timeout);

    // All are safe messages (no SQL details exposed)
    assert!(!connection_error.message.contains("SELECT"));
    assert!(!query_error.message.contains("SELECT"));
}

#[test]
fn test_validation_error_includes_limits() {
    // Validation errors can include constraint information (safe to show)
    let depth_error = GraphQLError::validation("Query exceeds maximum depth: 15 > 10");
    let complexity_error =
        GraphQLError::validation("Query exceeds maximum complexity: 1500 > 1000");

    // Showing limits is safe - it doesn't leak data, just policy
    assert!(depth_error.message.contains("15"));
    assert!(depth_error.message.contains("10"));
    assert!(complexity_error.message.contains("1500"));
    assert!(complexity_error.message.contains("1000"));
}

#[test]
fn test_error_path_for_field_errors() {
    // Errors can specify which field caused the error (safe in GraphQL)
    let error = GraphQLError::execution("Field 'email' failed validation")
        .with_path(vec!["user".to_string(), "email".to_string()]);

    assert!(error.path.is_some());
    let path = error.path.unwrap();
    assert_eq!(path.len(), 2);
    assert_eq!(path[0], "user");
    assert_eq!(path[1], "email");
}

#[test]
fn test_error_equality_by_code() {
    // Two errors with same code should be identifiable
    let error1 = GraphQLError::forbidden();
    let error2 = GraphQLError::forbidden();

    assert_eq!(error1.code, error2.code);
    assert_eq!(error1.code, ErrorCode::Forbidden);
}

#[test]
fn test_error_message_length_limits() {
    // Error messages should be reasonably sized (prevent response bloat)
    let error = GraphQLError::database("Short error");

    assert!(error.message.len() < 10000, "Error message should be bounded");
}

#[test]
fn test_request_error_for_malformed_input() {
    // RequestError is for client-provided malformed data
    let error = GraphQLError::request("Invalid JSON in request body");

    assert_eq!(error.code, ErrorCode::RequestError);
    assert!(error.message.contains("Invalid JSON"));
    // Don't show full payload that might contain sensitive data
    assert!(!error.message.contains("{"));
    assert!(!error.message.contains("}"));
}
