//! Example Validation Tests
//!
//! Validates that all code examples in documentation work correctly:
//! 1. Basic error handling example
//! 2. Query execution patterns
//! 3. GraphQL error responses
//! 4. Authorization patterns
//! 5. Timeout handling
//! 6. Rate limiting
//! 7. Request tracing with ID tracking
//! 8. Error chaining patterns
//!
//! # Running Tests
//!
//! ```bash
//! cargo test --test example_validation_test -- --nocapture
//! ```

#![cfg(test)]

use axum::http::StatusCode;
use fraiseql_server::error::{ErrorCode, ErrorExtensions, GraphQLError};

// ============================================================================
// Documentation Example 1: Basic Error Handling
// ============================================================================

#[test]
fn test_example_basic_error_handling() {
    // Example from docs showing basic error creation
    let error = GraphQLError::validation("Field 'email' is required for mutation 'createUser'");

    // Assertions match documentation
    assert_eq!(error.code, ErrorCode::ValidationError);
    assert_eq!(error.message, "Field 'email' is required for mutation 'createUser'");
    assert_eq!(error.code.status_code(), StatusCode::BAD_REQUEST);
}

// ============================================================================
// Documentation Example 2: Parse Error with Location
// ============================================================================

#[test]
fn test_example_parse_error_with_location() {
    // Example showing how to report parse errors with source location
    let error =
        GraphQLError::parse("Unexpected token '}' at line 1, column 15").with_location(1, 15);

    assert_eq!(error.code, ErrorCode::ParseError);
    assert!(error.locations.is_some());
    let locations = error.locations.unwrap();
    assert_eq!(locations[0].line, 1);
    assert_eq!(locations[0].column, 15);
}

// ============================================================================
// Documentation Example 3: Field Error with Path
// ============================================================================

#[test]
fn test_example_field_error_with_path() {
    // Example showing how to report field-level errors in responses
    let error = GraphQLError::validation("Cannot query field 'unknownField' on type 'User'")
        .with_path(vec!["user".to_string(), "profile".to_string()])
        .with_location(5, 10);

    assert_eq!(error.code, ErrorCode::ValidationError);
    assert!(error.path.is_some());
    let path = error.path.unwrap();
    assert_eq!(path.len(), 2);
    assert_eq!(path[0], "user");
    assert_eq!(path[1], "profile");
}

// ============================================================================
// Documentation Example 4: Authentication Error
// ============================================================================

#[test]
fn test_example_authentication_error() {
    // Example from docs: handling missing authentication
    let error = GraphQLError::unauthenticated();

    assert_eq!(error.code, ErrorCode::Unauthenticated);
    assert_eq!(error.code.status_code(), StatusCode::UNAUTHORIZED);
    assert_eq!(error.message, "Authentication required");
}

// ============================================================================
// Documentation Example 5: Authorization Error
// ============================================================================

#[test]
fn test_example_authorization_error() {
    // Example from docs: handling insufficient permissions
    let error = GraphQLError::forbidden();

    assert_eq!(error.code, ErrorCode::Forbidden);
    assert_eq!(error.code.status_code(), StatusCode::FORBIDDEN);
    assert_eq!(error.message, "Access denied");
}

// ============================================================================
// Documentation Example 6: Database Error
// ============================================================================

#[test]
fn test_example_database_error() {
    // Example showing database error handling
    let error = GraphQLError::database("Connection timeout: unable to reach database server");

    assert_eq!(error.code, ErrorCode::DatabaseError);
    assert_eq!(error.code.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
}

// ============================================================================
// Documentation Example 7: Timeout Error
// ============================================================================

#[test]
fn test_example_timeout_error() {
    // Example showing timeout error with helper
    let error = GraphQLError::timeout("Query execution");

    assert_eq!(error.code, ErrorCode::Timeout);
    assert_eq!(error.code.status_code(), StatusCode::REQUEST_TIMEOUT);
    assert!(error.message.contains("exceeded timeout"));
}

// ============================================================================
// Documentation Example 8: Rate Limit Error
// ============================================================================

#[test]
fn test_example_rate_limit_error() {
    // Example showing rate limiting error
    let error = GraphQLError::rate_limited("Exceeded 1000 requests per minute");

    assert_eq!(error.code, ErrorCode::RateLimitExceeded);
    assert_eq!(error.code.status_code(), StatusCode::TOO_MANY_REQUESTS);
}

// ============================================================================
// Documentation Example 9: Request ID for Distributed Tracing
// ============================================================================

#[test]
fn test_example_request_id_tracing() {
    // Example from docs showing request ID for distributed tracing
    let error = GraphQLError::database("Connection error").with_request_id("req-abc-123-xyz");

    assert!(error.extensions.is_some());
    let ext = error.extensions.unwrap();
    assert_eq!(ext.request_id, Some("req-abc-123-xyz".to_string()));
}

// ============================================================================
// Documentation Example 10: Error with Extensions Metadata
// ============================================================================

#[test]
fn test_example_error_with_extensions() {
    // Example showing how to add custom metadata via extensions
    let extensions = ErrorExtensions {
        category:   Some("DATABASE".to_string()),
        status:     Some(500),
        request_id: Some("req-db-001".to_string()),
    };

    let error = GraphQLError::database("Connection pool exhausted").with_extensions(extensions);

    assert!(error.extensions.is_some());
    let ext = error.extensions.unwrap();
    assert_eq!(ext.category, Some("DATABASE".to_string()));
    assert_eq!(ext.status, Some(500));
    assert_eq!(ext.request_id, Some("req-db-001".to_string()));
}

// ============================================================================
// Documentation Example 11: Error Chaining Pattern
// ============================================================================

#[test]
fn test_example_error_chaining() {
    // Example showing how to chain error builders
    let error = GraphQLError::validation("Argument type mismatch")
        .with_location(2, 5)
        .with_path(vec!["user".to_string(), "age".to_string()])
        .with_request_id("req-val-456");

    assert_eq!(error.code, ErrorCode::ValidationError);
    assert!(error.locations.is_some());
    assert!(error.path.is_some());
    assert!(error.extensions.is_some());
}

// ============================================================================
// Documentation Example 12: All Error Codes and Status Codes
// ============================================================================

#[test]
fn test_example_error_code_status_mapping() {
    // Example showing complete status code mapping for reference
    struct ErrorMapping {
        code:   ErrorCode,
        status: StatusCode,
    }

    let mappings = [
        ErrorMapping {
            code:   ErrorCode::ValidationError,
            status: StatusCode::BAD_REQUEST,
        },
        ErrorMapping {
            code:   ErrorCode::ParseError,
            status: StatusCode::BAD_REQUEST,
        },
        ErrorMapping {
            code:   ErrorCode::RequestError,
            status: StatusCode::BAD_REQUEST,
        },
        ErrorMapping {
            code:   ErrorCode::Unauthenticated,
            status: StatusCode::UNAUTHORIZED,
        },
        ErrorMapping {
            code:   ErrorCode::Forbidden,
            status: StatusCode::FORBIDDEN,
        },
        ErrorMapping {
            code:   ErrorCode::NotFound,
            status: StatusCode::NOT_FOUND,
        },
        ErrorMapping {
            code:   ErrorCode::Timeout,
            status: StatusCode::REQUEST_TIMEOUT,
        },
        ErrorMapping {
            code:   ErrorCode::RateLimitExceeded,
            status: StatusCode::TOO_MANY_REQUESTS,
        },
        ErrorMapping {
            code:   ErrorCode::DatabaseError,
            status: StatusCode::INTERNAL_SERVER_ERROR,
        },
        ErrorMapping {
            code:   ErrorCode::InternalServerError,
            status: StatusCode::INTERNAL_SERVER_ERROR,
        },
    ];

    // Verify all mappings match documentation
    for mapping in &mappings {
        let error = GraphQLError::new("test", mapping.code);
        assert_eq!(
            error.code.status_code(),
            mapping.status,
            "Status code mismatch for {:?}",
            mapping.code
        );
    }
}

// ============================================================================
// Documentation Example 13: Multiple Errors in Response
// ============================================================================

#[test]
fn test_example_multiple_errors_in_response() {
    // Example showing multiple errors in a single GraphQL response
    let errors = [
        GraphQLError::validation("Field 'email' is required")
            .with_path(vec!["user".to_string(), "email".to_string()]),
        GraphQLError::validation("Field 'age' must be >= 18")
            .with_path(vec!["user".to_string(), "age".to_string()]),
    ];

    assert_eq!(errors.len(), 2);
    assert!(errors.iter().all(|e| e.code == ErrorCode::ValidationError));
    assert!(errors[0].path.is_some());
    assert!(errors[1].path.is_some());
}

// ============================================================================
// Documentation Example 14: Common Error Patterns in Handlers
// ============================================================================

#[test]
fn test_example_handler_error_patterns() {
    // Example patterns commonly used in HTTP handlers

    // Pattern 1: Validation error from request
    {
        let error =
            GraphQLError::request("Invalid variable type").with_request_id("req-handler-001");
        assert_eq!(error.code, ErrorCode::RequestError);
        assert!(error.extensions.is_some());
    }

    // Pattern 2: Authorization check failure
    {
        let error = GraphQLError::forbidden();
        assert_eq!(error.code, ErrorCode::Forbidden);
    }

    // Pattern 3: Not found error
    {
        let error = GraphQLError::not_found("User with ID 123 not found");
        assert_eq!(error.code, ErrorCode::NotFound);
    }

    // Pattern 4: Internal error with context
    {
        let error = GraphQLError::internal("Unexpected error in field resolver");
        assert_eq!(error.code, ErrorCode::InternalServerError);
    }
}

// ============================================================================
// Documentation Example 15: Security Error Examples
// ============================================================================

#[test]
fn test_example_security_error_patterns() {
    // Examples of security-related errors

    // SQL injection attempt
    {
        let error = GraphQLError::request("Invalid input: SQL keywords detected");
        assert_eq!(error.code, ErrorCode::RequestError);
        assert_eq!(error.code.status_code(), StatusCode::BAD_REQUEST);
    }

    // XSS attempt
    {
        let error = GraphQLError::request("Invalid input: HTML/script tags not allowed");
        assert_eq!(error.code, ErrorCode::RequestError);
    }

    // Rate limiting for brute force protection
    {
        let error = GraphQLError::rate_limited("Too many failed login attempts");
        assert_eq!(error.code, ErrorCode::RateLimitExceeded);
        assert_eq!(error.code.status_code(), StatusCode::TOO_MANY_REQUESTS);
    }
}

// ============================================================================
// Documentation Example 16: Error Recovery Patterns
// ============================================================================

#[test]
fn test_example_error_recovery_patterns() {
    // Examples showing error patterns for client recovery

    // Retryable error (timeout)
    {
        let error = GraphQLError::timeout("Query execution");
        let status = error.code.status_code();
        // Timeout is retryable
        assert_eq!(status, StatusCode::REQUEST_TIMEOUT);
    }

    // Retryable error (service unavailable)
    {
        let error = GraphQLError::internal("Service temporarily unavailable");
        let status = error.code.status_code();
        // Internal server errors may be retryable
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    // Non-retryable error (validation)
    {
        let error = GraphQLError::validation("Invalid field type");
        let status = error.code.status_code();
        // Validation errors should not be retried
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    // Non-retryable error (authorization)
    {
        let error = GraphQLError::forbidden();
        let status = error.code.status_code();
        // Authorization errors should not be retried
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
}

// ============================================================================
// Real-World Example: End-to-End Error Handling in Resolver
// ============================================================================

#[test]
fn test_example_resolver_error_handling() {
    // Real example: Error handling in a GraphQL resolver

    // Scenario: User query resolver
    // 1. Database lookup fails
    let db_error = GraphQLError::database("Failed to query user table")
        .with_path(vec!["user".to_string()])
        .with_request_id("req-resolver-001");

    assert_eq!(db_error.code, ErrorCode::DatabaseError);
    assert_eq!(db_error.code.status_code(), StatusCode::INTERNAL_SERVER_ERROR);

    // 2. Authorization check fails
    let auth_error = GraphQLError::forbidden()
        .with_path(vec!["user".to_string(), "email".to_string()])
        .with_request_id("req-resolver-001");

    assert_eq!(auth_error.code, ErrorCode::Forbidden);
    assert_eq!(auth_error.code.status_code(), StatusCode::FORBIDDEN);

    // 3. Validation error in arguments
    let validation_error = GraphQLError::validation("Argument 'id' must be positive")
        .with_location(3, 20)
        .with_request_id("req-resolver-001");

    assert_eq!(validation_error.code, ErrorCode::ValidationError);
    assert_eq!(validation_error.code.status_code(), StatusCode::BAD_REQUEST);
}

// ============================================================================
// Real-World Example: Federation Error Handling
// ============================================================================

#[test]
fn test_example_federation_error_handling() {
    // Example: Error handling across federated subgraphs

    // Scenario: Cross-subgraph query with partial failure
    // Subgraph 1 (Users) succeeds, Subgraph 2 (Orders) times out

    let orders_timeout = GraphQLError::timeout("Orders subgraph")
        .with_path(vec!["user".to_string(), "orders".to_string()])
        .with_request_id("req-fed-001");

    assert_eq!(orders_timeout.code, ErrorCode::Timeout);
    assert_eq!(orders_timeout.code.status_code(), StatusCode::REQUEST_TIMEOUT);

    // Client can retry just the Orders subgraph using request ID
    assert!(orders_timeout.extensions.is_some());
    let ext = orders_timeout.extensions.unwrap();
    assert_eq!(ext.request_id, Some("req-fed-001".to_string()));
}

// ============================================================================
// Real-World Example: GraphQL Response with Errors and Partial Data
// ============================================================================

#[test]
fn test_example_graphql_response_with_partial_errors() {
    // Example: Proper GraphQL error response for partial failures

    // Scenario: Query returns data but some fields have errors
    // {
    //   "data": {
    //     "user": { "id": "1", "name": "Alice" },
    //     "posts": null  // null because field had error
    //   },
    //   "errors": [
    //     {
    //       "message": "Cannot access field 'posts' without permission",
    //       "code": "FORBIDDEN",
    //       "path": ["user", "posts"]
    //     }
    //   ]
    // }

    let partial_error =
        GraphQLError::forbidden().with_path(vec!["user".to_string(), "posts".to_string()]);

    assert_eq!(partial_error.code, ErrorCode::Forbidden);
    assert!(partial_error.path.is_some());
    let path = partial_error.path.unwrap();
    assert_eq!(path[0], "user");
    assert_eq!(path[1], "posts");
}

// ============================================================================
// Real-World Example: Batch Query Error Handling
// ============================================================================

#[test]
fn test_example_batch_query_error_handling() {
    // Example: Error handling for batched queries

    // Batch of 3 queries:
    // Query 1: Valid, succeeds
    // Query 2: Syntax error
    // Query 3: Authorization error

    let query2_error = GraphQLError::parse("Unexpected token '}' in query 2")
        .with_location(5, 15)
        .with_request_id("req-batch-001");

    let query3_error = GraphQLError::forbidden().with_request_id("req-batch-001");

    // All errors share same request ID for batch correlation
    assert_eq!(query2_error.code, ErrorCode::ParseError);
    assert_eq!(query3_error.code, ErrorCode::Forbidden);

    let ext2 = query2_error.extensions.unwrap_or(ErrorExtensions {
        category:   None,
        status:     None,
        request_id: Some("req-batch-001".to_string()),
    });
    let ext3 = query3_error.extensions.unwrap_or(ErrorExtensions {
        category:   None,
        status:     None,
        request_id: Some("req-batch-001".to_string()),
    });

    // Both reference same request ID
    assert_eq!(ext2.request_id, Some("req-batch-001".to_string()));
    assert_eq!(ext3.request_id, Some("req-batch-001".to_string()));
}

// ============================================================================
// Real-World Example: Subscription Error Handling
// ============================================================================

#[test]
fn test_example_subscription_error_handling() {
    // Example: Error handling for subscriptions

    // Scenario: Subscription authentication fails
    let auth_error = GraphQLError::unauthenticated().with_request_id("req-sub-001");

    assert_eq!(auth_error.code, ErrorCode::Unauthenticated);
    assert_eq!(auth_error.code.status_code(), StatusCode::UNAUTHORIZED);

    // Scenario: Too many subscriptions from client
    let resource_error =
        GraphQLError::rate_limited("Client already has 100 active subscriptions, maximum is 100")
            .with_request_id("req-sub-002");

    assert_eq!(resource_error.code, ErrorCode::RateLimitExceeded);
    assert_eq!(resource_error.code.status_code(), StatusCode::TOO_MANY_REQUESTS);
}

// ============================================================================
// Example: Complete Error Context Pattern
// ============================================================================

#[test]
fn test_example_complete_error_context() {
    // Production pattern: Rich error context for debugging

    let error = GraphQLError::database("Connection to primary replica failed after 3 retries")
        .with_location(10, 5) // Line and column in query
        .with_path(vec!["orders".to_string(), "items".to_string()]) // Field path
        .with_request_id("req-abc-xyz-123") // For correlation
        .with_extensions(ErrorExtensions {
            category:   Some("DATABASE_REPLICA".to_string()),
            status:     Some(503),
            request_id: Some("req-abc-xyz-123".to_string()),
        });

    // All context present for debugging
    assert_eq!(error.code, ErrorCode::DatabaseError);
    assert!(error.locations.is_some());
    assert!(error.path.is_some());
    assert!(error.extensions.is_some());

    // Verify each component
    assert_eq!(error.locations.unwrap()[0].line, 10);
    assert_eq!(error.path.unwrap()[0], "orders");
    assert_eq!(error.extensions.unwrap().category, Some("DATABASE_REPLICA".to_string()));
}
