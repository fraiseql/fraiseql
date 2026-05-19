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
#[non_exhaustive]
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
            // Spec §7.1.2: well-formed requests that fail GraphQL validation, parsing,
            // or APQ "not found" (signal for client to re-send with query body)
            // MUST return 2xx.
            Self::ValidationError | Self::ParseError | Self::PersistedQueryNotFound => {
                StatusCode::OK
            },
            // Truly malformed HTTP request (missing `query` field, unparseable JSON body),
            // APQ hash mismatch, forbidden queries, or missing trusted documents.
            Self::RequestError
            | Self::PersistedQueryMismatch
            | Self::ForbiddenQuery
            | Self::DocumentNotFound => StatusCode::BAD_REQUEST,
            Self::Unauthenticated => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict => StatusCode::CONFLICT,
            Self::RateLimitExceeded => StatusCode::TOO_MANY_REQUESTS,
            Self::Timeout => StatusCode::REQUEST_TIMEOUT,
            Self::InternalServerError | Self::DatabaseError => StatusCode::INTERNAL_SERVER_ERROR,
            Self::CircuitBreakerOpen => StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}

/// Error location in GraphQL query.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorLocation {
    /// Line number (1-indexed).
    pub line: usize,
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
            category: None,
            status: None,
            request_id: None,
            retry_after_secs: None,
            detail: None,
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
            category: Some("CIRCUIT_BREAKER".to_string()),
            status: Some(503),
            request_id: None,
            retry_after_secs: Some(retry_after_secs),
            detail: None,
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
