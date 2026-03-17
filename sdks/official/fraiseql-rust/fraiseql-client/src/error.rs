//! Error types for the FraiseQL client.

use thiserror::Error;

use crate::types::GraphQLErrorEntry;

/// All errors that can arise from the FraiseQL client.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum FraiseQLError {
    /// One or more errors in the GraphQL `errors` array.
    #[error("GraphQL error: {}", .errors.first().map_or("unknown", |e| e.message.as_str()))]
    GraphQL {
        /// The error entries from the response.
        errors: Vec<GraphQLErrorEntry>,
    },

    /// Transport-level error (connection refused, DNS failure, etc.)
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    /// The request exceeded the configured timeout.
    #[error("Request timed out after {timeout_ms}ms")]
    Timeout {
        /// The timeout duration in milliseconds.
        timeout_ms: u64,
    },

    /// Server returned HTTP 401 or 403.
    #[error("Authentication failed (HTTP {status_code})")]
    Authentication {
        /// The HTTP status code (401 or 403).
        status_code: u16,
    },

    /// Server returned HTTP 429 — rate limit exceeded.
    #[error("Rate limit exceeded")]
    RateLimit {
        /// Suggested retry delay, if provided by the server.
        retry_after: Option<std::time::Duration>,
    },
}

/// Convenience type alias for FraiseQL client results.
pub type Result<T> = std::result::Result<T, FraiseQLError>;
