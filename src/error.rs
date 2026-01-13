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

    /// Create a connection refused error (helpful message for debugging)
    pub fn connection_refused(host: &str, port: u16) -> Self {
        Error::Connection(format!(
            "failed to connect to {}:{}: connection refused. \
            Is Postgres running? Verify with: pg_isready -h {} -p {}",
            host, port, host, port
        ))
    }

    /// Create a protocol error with context
    pub fn protocol<S: Into<String>>(msg: S) -> Self {
        Error::Protocol(msg.into())
    }

    /// Create a SQL error with context
    pub fn sql<S: Into<String>>(msg: S) -> Self {
        Error::Sql(msg.into())
    }

    /// Create a schema validation error (query returned wrong columns)
    pub fn invalid_schema_columns(num_columns: usize) -> Self {
        Error::InvalidSchema(format!(
            "query returned {} columns instead of 1. \
            fraiseql-wire supports only: SELECT data FROM <view>. \
            See TROUBLESHOOTING.md#error-invalid-result-schema",
            num_columns
        ))
    }

    /// Create an invalid schema error with context
    pub fn invalid_schema<S: Into<String>>(msg: S) -> Self {
        Error::InvalidSchema(msg.into())
    }

    /// Create an authentication error with helpful message
    pub fn auth_failed(username: &str, reason: &str) -> Self {
        Error::Authentication(format!(
            "authentication failed for user '{}': {}. \
            Verify credentials with: psql -U {} -W",
            username, reason, username
        ))
    }

    /// Create a config error with helpful message
    pub fn config_invalid<S: Into<String>>(msg: S) -> Self {
        Error::Config(format!(
            "invalid configuration: {}. \
            Expected format: postgres://[user[:password]@][host[:port]]/[database]",
            msg.into()
        ))
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
    fn test_error_connection_refused() {
        let err = Error::connection_refused("localhost", 5432);
        let msg = err.to_string();
        assert!(msg.contains("connection refused"));
        assert!(msg.contains("Is Postgres running?"));
        assert!(msg.contains("localhost"));
        assert!(msg.contains("5432"));
    }

    #[test]
    fn test_error_invalid_schema_columns() {
        let err = Error::invalid_schema_columns(2);
        let msg = err.to_string();
        assert!(msg.contains("2 columns"));
        assert!(msg.contains("instead of 1"));
        assert!(msg.contains("SELECT data FROM"));
    }

    #[test]
    fn test_error_auth_failed() {
        let err = Error::auth_failed("postgres", "invalid password");
        let msg = err.to_string();
        assert!(msg.contains("postgres"));
        assert!(msg.contains("invalid password"));
        assert!(msg.contains("psql"));
    }

    #[test]
    fn test_error_config_invalid() {
        let err = Error::config_invalid("missing database name");
        let msg = err.to_string();
        assert!(msg.contains("invalid configuration"));
        assert!(msg.contains("postgres://"));
        assert!(msg.contains("missing database name"));
    }

    #[test]
    fn test_error_category() {
        assert_eq!(Error::connection("test").category(), "connection");
        assert_eq!(Error::sql("test").category(), "sql");
        assert_eq!(Error::Cancelled.category(), "cancelled");
        assert_eq!(Error::ConnectionClosed.category(), "connection_closed");
    }

    #[test]
    fn test_error_message_clarity() {
        // Verify error messages are clear and actionable
        let err = Error::connection_refused("example.com", 5432);
        let msg = err.to_string();

        // Should suggest a diagnostic command
        assert!(msg.contains("pg_isready"));

        // Should include the connection details
        assert!(msg.contains("example.com"));
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

    #[test]
    fn test_retriable_classification() {
        // Transient errors should be retriable
        assert!(Error::ConnectionClosed.is_retriable());
        assert!(Error::Io(io::Error::new(
            io::ErrorKind::ConnectionReset,
            "reset"
        ))
        .is_retriable());

        // Permanent errors should not be retriable
        assert!(!Error::auth_failed("user", "invalid password").is_retriable());
        assert!(!Error::sql("syntax error").is_retriable());
        assert!(!Error::invalid_schema_columns(3).is_retriable());
    }
}
