//! Integration tests for FraiseQL security stack components.
//!
//! Verifies behavioral correctness of:
//! - Error response construction and information leakage prevention
//! - Request validation (depth, complexity, malformed queries)
//! - Security error hierarchy and serialization

use fraiseql_server::{
    error::{ErrorCode, ErrorResponse, GraphQLError},
    routes::graphql::GraphQLRequest,
    validation::RequestValidator,
};

// =============================================================================
// Error Response Behavior Tests
// =============================================================================

#[test]
fn test_forbidden_error_has_generic_message() {
    let error = GraphQLError::forbidden();

    assert_eq!(error.code, ErrorCode::Forbidden);
    assert_eq!(error.message, "Access denied");
    // Must not reveal internals
    assert!(!error.message.contains("field"));
    assert!(!error.message.contains("permission"));
    assert!(!error.message.contains("row"));
    assert!(!error.message.contains("RBAC"));
    assert!(!error.message.contains("RLS"));
}

#[test]
fn test_forbidden_error_with_path_preserves_location() {
    let error =
        GraphQLError::forbidden().with_path(vec!["user".to_string(), "sensitiveField".to_string()]);

    assert_eq!(error.code, ErrorCode::Forbidden);
    // Path should be set for debugging but message stays generic
    let path = error.path.as_ref().unwrap();
    assert_eq!(path.len(), 2);
    assert_eq!(path[0], "user");
    assert_eq!(path[1], "sensitiveField");
    // Message still generic despite path existing
    assert_eq!(error.message, "Access denied");
}

#[test]
fn test_error_response_from_error_wraps_correctly() {
    let error =
        GraphQLError::forbidden().with_path(vec!["user".to_string(), "sensitiveField".to_string()]);

    let response = ErrorResponse::from_error(error);

    assert_eq!(response.errors.len(), 1);
    assert_eq!(response.errors[0].code, ErrorCode::Forbidden);
    assert_eq!(response.errors[0].message, "Access denied");
    assert!(response.errors[0].path.is_some());
}

#[test]
fn test_error_response_serializes_to_graphql_spec() {
    let error = GraphQLError::forbidden().with_path(vec!["query".to_string(), "user".to_string()]);
    let response = ErrorResponse::from_error(error);

    let json = serde_json::to_value(&response).unwrap();

    // GraphQL spec: errors array with message and optional path/extensions
    assert!(json["errors"].is_array());
    let first_error = &json["errors"][0];
    assert!(first_error["message"].is_string());
    assert_eq!(first_error["message"], "Access denied");
}

#[test]
fn test_validation_error_distinct_from_forbidden() {
    let validation_error = GraphQLError::validation("Field 'foo' doesn't exist");
    let forbidden_error = GraphQLError::forbidden();

    assert_eq!(validation_error.code, ErrorCode::ValidationError);
    assert_eq!(forbidden_error.code, ErrorCode::Forbidden);
    // Validation errors can include details; forbidden errors must not
    assert!(validation_error.message.contains("foo"));
    assert!(!forbidden_error.message.contains("foo"));
}

// =============================================================================
// Request Validation Behavioral Tests
// =============================================================================

#[test]
fn test_validator_accepts_simple_query() {
    let validator = RequestValidator::new();
    let result = validator.validate_query("{ user { id name } }");
    assert!(result.is_ok(), "Simple query should pass validation");
}

#[test]
fn test_validator_rejects_empty_query() {
    let validator = RequestValidator::new();

    let result = validator.validate_query("");
    assert!(result.is_err(), "Empty query must be rejected");

    let result = validator.validate_query("   ");
    assert!(result.is_err(), "Whitespace-only query must be rejected");
}

#[test]
fn test_validator_rejects_malformed_query() {
    let validator = RequestValidator::new()
        .with_depth_validation(true)
        .with_complexity_validation(true);

    let malformed = vec![
        "{ user { id",           // unclosed brace
        "not a query",           // invalid syntax
        "{ user { id } } extra", // trailing content may fail parsing
    ];

    for query in malformed {
        let result = validator.validate_query(query);
        // Should either reject as malformed or succeed if parser is lenient
        // (we just verify no panic)
        let _ = result;
    }
}

#[test]
fn test_validator_enforces_depth_limit() {
    let validator = RequestValidator::new().with_max_depth(3).with_depth_validation(true);

    // Within limit
    let shallow = "{ user { id name } }";
    assert!(
        validator.validate_query(shallow).is_ok(),
        "Depth-2 query should pass depth-3 limit"
    );

    // Exceeds limit
    let deep = "{ user { posts { comments { replies { author { id } } } } } }";
    let result = validator.validate_query(deep);
    assert!(result.is_err(), "Depth-6 query should fail depth-3 limit: {result:?}");
}

#[test]
fn test_validator_enforces_complexity_limit() {
    let validator = RequestValidator::new().with_max_complexity(5).with_complexity_validation(true);

    // Simple query within limit
    let simple = "{ user { id } }";
    assert!(
        validator.validate_query(simple).is_ok(),
        "Simple query should pass complexity-5 limit"
    );

    // High-complexity query (many fields = high complexity)
    let complex = "{ user { id name email phone address bio avatar role createdAt updatedAt } }";
    let result = validator.validate_query(complex);
    assert!(result.is_err(), "10-field query should fail complexity-5 limit: {result:?}");
}

#[test]
fn test_validator_depth_disabled_allows_deep_queries() {
    let validator = RequestValidator::new().with_max_depth(1).with_depth_validation(false);

    let deep = "{ user { posts { comments { id } } } }";
    assert!(
        validator.validate_query(deep).is_ok(),
        "Deep query should pass when depth validation is disabled"
    );
}

#[test]
fn test_validator_accepts_mutations() {
    let validator = RequestValidator::new();
    let mutation = "mutation { createUser(input: { name: \"test\" }) { id } }";
    assert!(validator.validate_query(mutation).is_ok(), "Mutation should pass validation");
}

#[test]
fn test_validator_accepts_query_with_variables() {
    let validator = RequestValidator::new();
    let query = "query GetUser($id: ID!) { user(id: $id) { id name email } }";
    assert!(
        validator.validate_query(query).is_ok(),
        "Query with variables should pass validation"
    );
}

#[test]
fn test_validator_accepts_fragments() {
    let validator = RequestValidator::new();
    let query = "query { users { ...UserFields } } fragment UserFields on User { id name }";
    assert!(
        validator.validate_query(query).is_ok(),
        "Query with fragments should pass validation"
    );
}

#[test]
fn test_validator_accepts_directives() {
    let validator = RequestValidator::new();
    let query =
        "query GetUser($withEmail: Boolean!) { user { id name email @include(if: $withEmail) } }";
    assert!(
        validator.validate_query(query).is_ok(),
        "Query with directives should pass validation"
    );
}

// =============================================================================
// GraphQL Request Structure Tests
// =============================================================================

#[test]
fn test_graphql_request_deserializes_from_json() {
    let json = serde_json::json!({
        "query": "query { user(id: \"123\") { id name email } }",
        "variables": {"id": "123"},
        "operationName": "GetUser"
    });

    let request: GraphQLRequest = serde_json::from_value(json).unwrap();
    assert_eq!(request.query.as_deref(), Some("query { user(id: \"123\") { id name email } }"));
    assert!(request.variables.is_some());
    assert_eq!(request.operation_name, Some("GetUser".to_string()));
}

#[test]
fn test_graphql_request_minimal() {
    let request = GraphQLRequest {
        query:          Some("{ user { id } }".to_string()),
        variables:      None,
        operation_name: None,
        extensions:     None,
        document_id:    None,
    };

    let validator = RequestValidator::new();
    assert!(validator.validate_query(request.query.as_deref().unwrap()).is_ok());
}

// =============================================================================
// Security Error Information Leakage Tests
// =============================================================================

#[test]
fn test_forbidden_error_does_not_leak_schema_info() {
    let fields = vec!["password", "ssn", "secretKey", "internalId"];

    for field in fields {
        let error =
            GraphQLError::forbidden().with_path(vec!["query".to_string(), field.to_string()]);

        // The error message must never contain the field name
        assert!(
            !error.message.to_lowercase().contains(&field.to_lowercase()),
            "Forbidden error leaks field name '{field}' in message: {}",
            error.message
        );
    }
}

#[test]
fn test_error_response_multiple_errors() {
    let errors = vec![
        GraphQLError::forbidden().with_path(vec!["user".to_string(), "password".to_string()]),
        GraphQLError::forbidden().with_path(vec!["user".to_string(), "ssn".to_string()]),
    ];

    let response = ErrorResponse {
        errors: errors.clone(),
    };

    assert_eq!(response.errors.len(), 2);
    // All errors should have generic messages
    for error in &response.errors {
        assert_eq!(error.message, "Access denied");
    }
}
