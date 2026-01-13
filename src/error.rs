//! Error types for fraiseql-wire

use std::io;
use thiserror::Error;

/// Main error type for fraiseql-wire operations
#[derive(Debug, Error)]
pub enum Error {
    /// Connection error
    #[error("connection error: {0}")]
    Connection(String),

    /// Authentication error
    #[error("authentication failed: {0}")]
    Authentication(String),

    /// Protocol violation
    #[error("protocol error: {0}")]
    Protocol(String),

    /// SQL execution error
    #[error("sql error: {0}")]
    Sql(String),

    /// JSON decoding error
    #[error("json decode error: {0}")]
    JsonDecode(#[from] serde_json::Error),

    /// I/O error
    #[error("io error: {0}")]
    Io(#[from] io::Error),

    /// Invalid configuration
    #[error("invalid configuration: {0}")]
    Config(String),

    /// Query cancelled by client
    #[error("query cancelled")]
    Cancelled,

    /// Invalid result schema (not single `data` column)
    #[error("invalid result schema: {0}")]
    InvalidSchema(String),

    /// Connection already in use
    #[error("connection busy: {0}")]
    ConnectionBusy(String),

    /// Invalid connection state
    #[error("invalid connection state: expected {expected}, got {actual}")]
    InvalidState {
        /// Expected state
        expected: String,
        /// Actual state
        actual: String,
    },

    /// Connection closed
    #[error("connection closed")]
    ConnectionClosed,
}

/// Result type alias using fraiseql-wire Error
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Create a connection error with context
    pub fn connection<S: Into<String>>(msg: S) -> Self {
        Error::Connection(msg.into())
    }

    /// Create a protocol error with context
    pub fn protocol<S: Into<String>>(msg: S) -> Self {
        Error::Protocol(msg.into())
    }

    /// Create a SQL error with context
    pub fn sql<S: Into<String>>(msg: S) -> Self {
        Error::Sql(msg.into())
    }

    /// Create an invalid schema error with context
    pub fn invalid_schema<S: Into<String>>(msg: S) -> Self {
        Error::InvalidSchema(msg.into())
    }

    /// Check if error is retriable (transient)
    ///
    /// Retriable errors typically indicate temporary issues that may succeed on retry:
    /// - I/O errors (network timeouts, etc.)
    /// - Connection closed (can reconnect)
    ///
    /// Non-retriable errors indicate permanent problems:
    /// - Invalid schema (won't change between attempts)
    /// - Invalid configuration (needs user intervention)
    /// - SQL errors (query is invalid)
    pub fn is_retriable(&self) -> bool {
        matches!(self, Error::Io(_) | Error::ConnectionClosed)
    }

    /// Get error category for observability and logging
    ///
    /// Used to categorize errors for metrics, tracing, and error handling decisions.
    pub fn category(&self) -> &'static str {
        match self {
            Error::Connection(_) => "connection",
            Error::Authentication(_) => "authentication",
            Error::Protocol(_) => "protocol",
            Error::Sql(_) => "sql",
            Error::JsonDecode(_) => "json_decode",
            Error::Io(_) => "io",
            Error::Config(_) => "config",
            Error::Cancelled => "cancelled",
            Error::InvalidSchema(_) => "invalid_schema",
            Error::ConnectionBusy(_) => "connection_busy",
            Error::InvalidState { .. } => "invalid_state",
            Error::ConnectionClosed => "connection_closed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_helpers() {
        let conn_err = Error::connection("failed to connect");
        assert!(matches!(conn_err, Error::Connection(_)));

        let proto_err = Error::protocol("unexpected message");
        assert!(matches!(proto_err, Error::Protocol(_)));

        let sql_err = Error::sql("syntax error");
        assert!(matches!(sql_err, Error::Sql(_)));

        let schema_err = Error::invalid_schema("expected single column");
        assert!(matches!(schema_err, Error::InvalidSchema(_)));
    }

    #[test]
    fn test_error_category() {
        assert_eq!(Error::connection("test").category(), "connection");
        assert_eq!(Error::sql("test").category(), "sql");
        assert_eq!(Error::Cancelled.category(), "cancelled");
        assert_eq!(Error::ConnectionClosed.category(), "connection_closed");
    }

    #[test]
    fn test_is_retriable() {
        assert!(Error::ConnectionClosed.is_retriable());
        assert!(Error::Io(io::Error::new(
            io::ErrorKind::TimedOut,
            "timeout"
        ))
        .is_retriable());

        assert!(!Error::connection("test").is_retriable());
        assert!(!Error::sql("test").is_retriable());
        assert!(!Error::invalid_schema("test").is_retriable());
    }
}
