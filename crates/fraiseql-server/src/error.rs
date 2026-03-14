//! GraphQL error response handling.
//!
//! Implements GraphQL spec-compliant error responses with:
//! - Error codes for client-side handling
//! - Location tracking in queries
//! - Extensions for custom error data

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

/// GraphQL error code enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// Validation error.
    ValidationError,
    /// Parse error.
    ParseError,
    /// Request error.
    RequestError,
    /// Authentication required.
    Unauthenticated,
    /// Access denied.
    Forbidden,
    /// Internal server error.
    InternalServerError,
    /// Database error.
    DatabaseError,
    /// Timeout error.
    Timeout,
    /// Rate limit exceeded.
    RateLimitExceeded,
    /// Not found.
    NotFound,
    /// Conflict.
    Conflict,
    /// Circuit breaker open — federation entity temporarily unavailable.
    CircuitBreakerOpen,
    /// Persisted query not found — client must re-send the full query body.
    PersistedQueryNotFound,
    /// Persisted query hash mismatch — SHA-256 of body does not match provided hash.
    PersistedQueryMismatch,
    /// Raw query forbidden — trusted documents strict mode requires a documentId.
    ForbiddenQuery,
    /// Document not found — the provided documentId is not in the trusted manifest.
    DocumentNotFound,
}

impl ErrorCode {
    /// Get HTTP status code for this error.
    ///
    /// Follows the [GraphQL over HTTP spec](https://graphql.github.io/graphql-over-http/):
    /// a well-formed GraphQL request that fails validation or parsing returns **200 OK**
    /// with `{"errors": [...]}` in the body — never a 4xx — so that standard HTTP clients
    /// can read the error message rather than raising a transport-level exception.
    ///
    /// Only [`RequestError`](Self::RequestError) uses 400, because it indicates a truly
    /// malformed HTTP request (missing `query` field, unreadable JSON body) that was never
    /// a valid GraphQL request to begin with.
    #[must_use]
    pub const fn status_code(self) -> StatusCode {
        match self {
            // Spec §7.1.2: well-formed requests that fail GraphQL validation or parsing
            // MUST return 2xx.  200 is the correct status — the request was received and
            // processed; it just failed at the GraphQL layer.
            Self::ValidationError | Self::ParseError => StatusCode::OK,
            // Truly malformed HTTP request (missing `query` field, unparseable JSON body).
            Self::RequestError => StatusCode::BAD_REQUEST,
            Self::Unauthenticated => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict => StatusCode::CONFLICT,
            Self::RateLimitExceeded => StatusCode::TOO_MANY_REQUESTS,
            Self::Timeout => StatusCode::REQUEST_TIMEOUT,
            Self::InternalServerError | Self::DatabaseError => StatusCode::INTERNAL_SERVER_ERROR,
            Self::CircuitBreakerOpen => StatusCode::SERVICE_UNAVAILABLE,
            // APQ protocol: "not found" is a signal for the client to re-send with query body.
            Self::PersistedQueryNotFound => StatusCode::OK,
            Self::PersistedQueryMismatch => StatusCode::BAD_REQUEST,
            Self::ForbiddenQuery | Self::DocumentNotFound => StatusCode::BAD_REQUEST,
        }
    }
}

/// Error location in GraphQL query.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorLocation {
    /// Line number (1-indexed).
    pub line:   usize,
    /// Column number (1-indexed).
    pub column: usize,
}

/// GraphQL error following spec.
#[derive(Debug, Clone, Serialize)]
pub struct GraphQLError {
    /// Error message.
    pub message: String,

    /// Error code for client handling.
    pub code: ErrorCode,

    /// Location in query where error occurred.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locations: Option<Vec<ErrorLocation>>,

    /// Path to field that caused error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<Vec<String>>,

    /// Additional error information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<ErrorExtensions>,
}

/// Additional error context and debugging information.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorExtensions {
    /// Error category.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,

    /// HTTP status code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,

    /// Request ID for tracking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,

    /// Seconds until the client may retry (set for `CircuitBreakerOpen` errors).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_secs: Option<u64>,

    /// Internal error detail (SQL fragment, stack trace, etc.).
    ///
    /// Stripped from responses when error sanitization is enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// GraphQL response with errors.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Errors that occurred.
    pub errors: Vec<GraphQLError>,
}

impl GraphQLError {
    /// Create a new GraphQL error.
    pub fn new(message: impl Into<String>, code: ErrorCode) -> Self {
        Self {
            message: message.into(),
            code,
            locations: None,
            path: None,
            extensions: None,
        }
    }

    /// Add location to error.
    #[must_use]
    pub fn with_location(mut self, line: usize, column: usize) -> Self {
        self.locations = Some(vec![ErrorLocation { line, column }]);
        self
    }

    /// Add path to error.
    #[must_use]
    pub fn with_path(mut self, path: Vec<String>) -> Self {
        self.path = Some(path);
        self
    }

    /// Add extensions to error.
    #[must_use]
    pub fn with_extensions(mut self, extensions: ErrorExtensions) -> Self {
        self.extensions = Some(extensions);
        self
    }

    /// Add request ID for distributed tracing.
    #[must_use]
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        let request_id = request_id.into();
        let extensions = self.extensions.take().unwrap_or(ErrorExtensions {
            category:         None,
            status:           None,
            request_id:       None,
            retry_after_secs: None,
            detail:           None,
        });

        self.extensions = Some(ErrorExtensions {
            request_id: Some(request_id),
            ..extensions
        });
        self
    }

    /// Validation error.
    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(message, ErrorCode::ValidationError)
    }

    /// Parse error with hint for common syntax issues.
    pub fn parse(message: impl Into<String>) -> Self {
        Self::new(message, ErrorCode::ParseError)
    }

    /// Request error with validation details.
    pub fn request(message: impl Into<String>) -> Self {
        Self::new(message, ErrorCode::RequestError)
    }

    /// Database error - includes connection, timeout, and query errors.
    pub fn database(message: impl Into<String>) -> Self {
        Self::new(message, ErrorCode::DatabaseError)
    }

    /// Internal server error - unexpected conditions.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(message, ErrorCode::InternalServerError)
    }

    /// Execution error during GraphQL resolver execution.
    ///
    /// # Deprecation
    ///
    /// Prefer [`GraphQLError::from_fraiseql_error`] on the hot path; it preserves
    /// the specific error variant so clients and the sanitizer receive the correct code.
    /// This method remains for ad-hoc internal errors that do not originate from a
    /// `FraiseQLError`.
    #[doc(hidden)]
    #[must_use]
    pub fn execution(message: &str) -> Self {
        Self::new(message, ErrorCode::InternalServerError)
    }

    /// Unauthenticated error - authentication token is missing or invalid.
    #[must_use]
    pub fn unauthenticated() -> Self {
        Self::new("Authentication required", ErrorCode::Unauthenticated)
    }

    /// Forbidden error - user lacks permission to access resource.
    #[must_use]
    pub fn forbidden() -> Self {
        Self::new("Access denied", ErrorCode::Forbidden)
    }

    /// Not found error - requested resource does not exist.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(message, ErrorCode::NotFound)
    }

    /// Timeout error - operation took too long and was cancelled.
    pub fn timeout(operation: impl Into<String>) -> Self {
        Self::new(format!("{} exceeded timeout", operation.into()), ErrorCode::Timeout)
    }

    /// Rate limit error - too many requests from client.
    pub fn rate_limited(message: impl Into<String>) -> Self {
        Self::new(message, ErrorCode::RateLimitExceeded)
    }

    /// Construct a typed [`GraphQLError`] from a [`fraiseql_core::error::FraiseQLError`] executor
    /// error.
    ///
    /// Maps specific core error variants to their closest HTTP-semantic equivalent,
    /// preserving type information for correct client handling and sanitizer routing.
    #[must_use]
    pub fn from_fraiseql_error(err: &fraiseql_core::error::FraiseQLError) -> Self {
        use fraiseql_core::error::FraiseQLError as E;
        match err {
            E::Database { .. } | E::ConnectionPool { .. } => Self::database(err.to_string()),
            E::Parse { .. } => Self::parse(err.to_string()),
            E::Validation { .. } | E::UnknownField { .. } | E::UnknownType { .. } => {
                Self::validation(err.to_string())
            },
            E::NotFound { .. } => Self::not_found(err.to_string()),
            E::Conflict { .. } => Self::new(err.to_string(), ErrorCode::Conflict),
            E::Authorization { .. } => Self::forbidden(),
            E::Authentication { .. } => Self::unauthenticated(),
            E::Timeout { .. } => Self::new(err.to_string(), ErrorCode::Timeout),
            E::RateLimited { message, .. } => Self::rate_limited(message.clone()),
            // Cancelled, Configuration, Internal, and any future variants
            _ => Self::internal(err.to_string()),
        }
    }

    /// Persisted query not found — client must re-send the full query body.
    #[must_use]
    pub fn persisted_query_not_found() -> Self {
        Self::new("PersistedQueryNotFound", ErrorCode::PersistedQueryNotFound)
    }

    /// Persisted query hash mismatch — SHA-256 of body does not match the provided hash.
    #[must_use]
    pub fn persisted_query_mismatch() -> Self {
        Self::new("provided sha does not match query", ErrorCode::PersistedQueryMismatch)
    }

    /// Raw query forbidden — trusted documents strict mode requires a documentId.
    #[must_use]
    pub fn forbidden_query() -> Self {
        Self::new(
            "Raw queries are not permitted. Send a documentId instead.",
            ErrorCode::ForbiddenQuery,
        )
    }

    /// Document not found — the provided documentId is not in the trusted manifest.
    pub fn document_not_found(doc_id: impl Into<String>) -> Self {
        Self::new(format!("Unknown document: {}", doc_id.into()), ErrorCode::DocumentNotFound)
    }

    /// Circuit breaker open — federation entity temporarily unavailable.
    ///
    /// The response will carry a `Retry-After` header set to `retry_after_secs`.
    #[must_use]
    pub fn circuit_breaker_open(entity: &str, retry_after_secs: u64) -> Self {
        Self::new(
            format!(
                "Federation entity '{entity}' is temporarily unavailable. \
                 Please retry after {retry_after_secs} seconds."
            ),
            ErrorCode::CircuitBreakerOpen,
        )
        .with_extensions(ErrorExtensions {
            category:         Some("CIRCUIT_BREAKER".to_string()),
            status:           Some(503),
            request_id:       None,
            retry_after_secs: Some(retry_after_secs),
            detail:           None,
        })
    }
}

impl ErrorResponse {
    /// Create new error response.
    #[must_use]
    pub const fn new(errors: Vec<GraphQLError>) -> Self {
        Self { errors }
    }

    /// Create from single error.
    #[must_use]
    pub fn from_error(error: GraphQLError) -> Self {
        Self {
            errors: vec![error],
        }
    }
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        let status = self
            .errors
            .first()
            .map_or(StatusCode::INTERNAL_SERVER_ERROR, |e| e.code.status_code());

        let retry_after = self
            .errors
            .first()
            .and_then(|e| e.extensions.as_ref())
            .and_then(|ext| ext.retry_after_secs);

        let mut response = (status, Json(self)).into_response();

        if let Some(secs) = retry_after {
            if let Ok(value) = secs.to_string().parse() {
                response.headers_mut().insert(axum::http::header::RETRY_AFTER, value);
            }
        }

        response
    }
}

impl From<GraphQLError> for ErrorResponse {
    fn from(error: GraphQLError) -> Self {
        Self::from_error(error)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use super::*;

    #[test]
    fn test_error_serialization() {
        let error = GraphQLError::validation("Invalid query")
            .with_location(1, 5)
            .with_path(vec!["user".to_string(), "id".to_string()]);

        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("Invalid query"));
        assert!(json.contains("VALIDATION_ERROR"));
        assert!(json.contains("\"line\":1"));
    }

    #[test]
    fn test_error_response_serialization() {
        let response = ErrorResponse::new(vec![
            GraphQLError::validation("Field not found"),
            GraphQLError::database("Connection timeout"),
        ]);

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("Field not found"));
        assert!(json.contains("Connection timeout"));
    }

    #[test]
    fn test_error_code_status_codes() {
        // GraphQL-over-HTTP spec: validation/parse errors → 200 (client must read the body)
        assert_eq!(ErrorCode::ValidationError.status_code(), StatusCode::OK);
        assert_eq!(ErrorCode::ParseError.status_code(), StatusCode::OK);
        // Truly malformed HTTP request → 400
        assert_eq!(ErrorCode::RequestError.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(ErrorCode::Unauthenticated.status_code(), StatusCode::UNAUTHORIZED);
        assert_eq!(ErrorCode::Forbidden.status_code(), StatusCode::FORBIDDEN);
        assert_eq!(ErrorCode::DatabaseError.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(ErrorCode::CircuitBreakerOpen.status_code(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn test_circuit_breaker_open_error() {
        let error = GraphQLError::circuit_breaker_open("Product", 30);
        assert_eq!(error.code, ErrorCode::CircuitBreakerOpen);
        assert!(error.message.contains("Product"));
        assert!(error.message.contains("30"));
        let ext = error.extensions.unwrap();
        assert_eq!(ext.retry_after_secs, Some(30));
        assert_eq!(ext.category, Some("CIRCUIT_BREAKER".to_string()));
    }

    #[test]
    fn test_circuit_breaker_response_has_retry_after_header() {
        use axum::response::IntoResponse;

        let response = ErrorResponse::from_error(GraphQLError::circuit_breaker_open("User", 60))
            .into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let retry_after = response.headers().get(axum::http::header::RETRY_AFTER);
        assert_eq!(retry_after.and_then(|v| v.to_str().ok()), Some("60"));
    }

    #[test]
    fn test_from_fraiseql_error_database_maps_to_database_code() {
        use fraiseql_core::error::FraiseQLError;
        let err = FraiseQLError::Database {
            message:   "relation \"users\" does not exist".into(),
            sql_state: None,
        };
        let graphql_err = GraphQLError::from_fraiseql_error(&err);
        assert_eq!(graphql_err.code, ErrorCode::DatabaseError);
    }

    #[test]
    fn test_from_fraiseql_error_validation_maps_to_validation_code() {
        use fraiseql_core::error::FraiseQLError;
        let err = FraiseQLError::Validation {
            message: "field 'id' is required".into(),
            path:    None,
        };
        let graphql_err = GraphQLError::from_fraiseql_error(&err);
        assert_eq!(graphql_err.code, ErrorCode::ValidationError);
    }

    #[test]
    fn test_from_fraiseql_error_not_found_maps_to_not_found_code() {
        use fraiseql_core::error::FraiseQLError;
        let err = FraiseQLError::NotFound {
            resource_type: "User".into(),
            identifier:    "123".into(),
        };
        let graphql_err = GraphQLError::from_fraiseql_error(&err);
        assert_eq!(graphql_err.code, ErrorCode::NotFound);
    }

    #[test]
    fn test_from_fraiseql_error_authorization_maps_to_forbidden() {
        use fraiseql_core::error::FraiseQLError;
        let err = FraiseQLError::Authorization {
            message:  "insufficient permissions".into(),
            action:   Some("write".into()),
            resource: Some("User".into()),
        };
        let graphql_err = GraphQLError::from_fraiseql_error(&err);
        assert_eq!(graphql_err.code, ErrorCode::Forbidden);
    }

    #[test]
    fn test_from_fraiseql_error_authentication_maps_to_unauthenticated() {
        use fraiseql_core::error::FraiseQLError;
        let err = FraiseQLError::Authentication {
            message: "token expired".into(),
        };
        let graphql_err = GraphQLError::from_fraiseql_error(&err);
        assert_eq!(graphql_err.code, ErrorCode::Unauthenticated);
    }

    #[test]
    fn test_error_extensions() {
        let extensions = ErrorExtensions {
            category:         Some("VALIDATION".to_string()),
            status:           Some(400),
            request_id:       Some("req-123".to_string()),
            retry_after_secs: None,
            detail:           None,
        };

        let error = GraphQLError::validation("Invalid").with_extensions(extensions);
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("VALIDATION"));
        assert!(json.contains("req-123"));
    }

    // =========================================================================
    // Complete ErrorCode → HTTP status coverage
    // =========================================================================

    #[test]
    fn test_all_error_codes_have_expected_status() {
        // Every variant must map to an explicit HTTP status
        // GraphQL-over-HTTP spec §7.1.2: parse/validation errors on well-formed requests → 200
        assert_eq!(ErrorCode::ParseError.status_code(), StatusCode::OK);
        assert_eq!(ErrorCode::RequestError.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(ErrorCode::NotFound.status_code(), StatusCode::NOT_FOUND);
        assert_eq!(ErrorCode::Conflict.status_code(), StatusCode::CONFLICT);
        assert_eq!(ErrorCode::RateLimitExceeded.status_code(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(ErrorCode::Timeout.status_code(), StatusCode::REQUEST_TIMEOUT);
        assert_eq!(ErrorCode::InternalServerError.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(ErrorCode::PersistedQueryMismatch.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(ErrorCode::ForbiddenQuery.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(ErrorCode::DocumentNotFound.status_code(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_persisted_query_not_found_maps_to_200() {
        // APQ protocol: "not found" signals the client to re-send with the full query body.
        // Returning 200 OK is spec-required for the APQ flow.
        assert_eq!(ErrorCode::PersistedQueryNotFound.status_code(), StatusCode::OK);

        use axum::response::IntoResponse;
        let response =
            ErrorResponse::from_error(GraphQLError::persisted_query_not_found()).into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }

    // =========================================================================
    // FraiseQLError → GraphQLError conversion coverage
    // =========================================================================

    #[test]
    fn test_from_fraiseql_timeout_maps_to_timeout_code() {
        use fraiseql_core::error::FraiseQLError;
        let err = FraiseQLError::Timeout {
            timeout_ms: 5000,
            query:      Some("{ users { id } }".into()),
        };
        let graphql_err = GraphQLError::from_fraiseql_error(&err);
        assert_eq!(graphql_err.code, ErrorCode::Timeout);
    }

    #[test]
    fn test_from_fraiseql_rate_limited_maps_to_rate_limit_code() {
        use fraiseql_core::error::FraiseQLError;
        let err = FraiseQLError::RateLimited {
            message:          "too many requests".into(),
            retry_after_secs: 60,
        };
        let graphql_err = GraphQLError::from_fraiseql_error(&err);
        assert_eq!(graphql_err.code, ErrorCode::RateLimitExceeded);
    }

    #[test]
    fn test_from_fraiseql_conflict_maps_to_conflict_code() {
        use fraiseql_core::error::FraiseQLError;
        let err = FraiseQLError::Conflict {
            message: "unique constraint violated".into(),
        };
        let graphql_err = GraphQLError::from_fraiseql_error(&err);
        assert_eq!(graphql_err.code, ErrorCode::Conflict);
    }

    #[test]
    fn test_from_fraiseql_parse_maps_to_parse_code() {
        use fraiseql_core::error::FraiseQLError;
        let err = FraiseQLError::Parse {
            message:  "unexpected token".into(),
            location: "line 1, col 5".into(),
        };
        let graphql_err = GraphQLError::from_fraiseql_error(&err);
        assert_eq!(graphql_err.code, ErrorCode::ParseError);
    }

    #[test]
    fn test_from_fraiseql_internal_maps_to_internal_code() {
        use fraiseql_core::error::FraiseQLError;
        let err = FraiseQLError::Internal {
            message: "unexpected nil pointer".into(),
            source:  None,
        };
        let graphql_err = GraphQLError::from_fraiseql_error(&err);
        assert_eq!(graphql_err.code, ErrorCode::InternalServerError);
    }

    // =========================================================================
    // HTTP response structure tests
    // =========================================================================

    #[test]
    fn test_timeout_response_has_correct_status() {
        use axum::response::IntoResponse;
        let response =
            ErrorResponse::from_error(GraphQLError::new("timed out", ErrorCode::Timeout))
                .into_response();
        assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);
    }

    #[test]
    fn test_rate_limit_response_has_correct_status() {
        use axum::response::IntoResponse;
        let response = ErrorResponse::from_error(GraphQLError::rate_limited("too many requests"))
            .into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[test]
    fn test_not_found_response_has_correct_status() {
        use axum::response::IntoResponse;
        let response = ErrorResponse::from_error(GraphQLError::not_found("resource not found"))
            .into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    // =========================================================================
    // GraphQL-over-HTTP spec compliance: validation/parse errors → 200
    // =========================================================================

    /// Complexity and depth rejections must return HTTP 200, not 400.
    ///
    /// Per GraphQL-over-HTTP spec §7.1.2, a well-formed request that fails GraphQL
    /// validation (including complexity/depth limits) must produce a 200 response
    /// with `{"errors": [...]}`.  Returning 400 causes standard HTTP clients
    /// (urllib, fetch, axios) to raise exceptions rather than reading the error body,
    /// making it impossible to distinguish a transport failure from a validation failure.
    #[test]
    fn test_complexity_rejection_returns_200() {
        use axum::response::IntoResponse;
        let response = ErrorResponse::from_error(GraphQLError::validation(
            "Query exceeds maximum complexity: 121 > 100",
        ))
        .into_response();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "complexity validation errors must return HTTP 200 per GraphQL-over-HTTP spec"
        );
    }

    #[test]
    fn test_depth_rejection_returns_200() {
        use axum::response::IntoResponse;
        let response = ErrorResponse::from_error(GraphQLError::validation(
            "Query exceeds maximum depth: 16 > 15",
        ))
        .into_response();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "depth validation errors must return HTTP 200 per GraphQL-over-HTTP spec"
        );
    }

    #[test]
    fn test_parse_error_returns_200() {
        use axum::response::IntoResponse;
        let response =
            ErrorResponse::from_error(GraphQLError::parse("unexpected token '}'")).into_response();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "GraphQL parse errors must return HTTP 200 per GraphQL-over-HTTP spec"
        );
    }
}
