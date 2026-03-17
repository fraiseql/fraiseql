//! Error types for fraiseql-wire

use std::io;
use thiserror::Error;

/// Main error type for fraiseql-wire operations
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum WireError {
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

    /// Type deserialization error
    ///
    /// Occurs when a row cannot be deserialized into the target type.
    /// This is a consumer-side error that includes the type name and serde details.
    #[error("deserialization error for type '{type_name}': {details}")]
    Deserialization {
        /// Name of the type we were deserializing to
        type_name: String,
        /// Details from `serde_json` about what went wrong
        details: String,
    },

    /// Memory limit exceeded
    ///
    /// **Terminal error**: The consumer cannot keep pace with data arrival.
    ///
    /// Occurs when estimated buffered memory exceeds the configured maximum.
    /// This indicates the consumer is too slow relative to data arrival rate.
    ///
    /// NOT retriable: Retrying the same query with the same consumer will hit the same limit.
    ///
    /// Solutions:
    /// 1. Increase consumer throughput (faster `.next()` polling)
    /// 2. Reduce items in flight (configure lower `chunk_size`)
    /// 3. Remove memory limit (use unbounded mode)
    /// 4. Use different transport (consider `tokio-postgres` for flexibility)
    #[error("memory limit exceeded: {estimated_memory} bytes buffered > {limit} bytes limit")]
    MemoryLimitExceeded {
        /// Configured memory limit in bytes
        limit: usize,
        /// Current estimated memory in bytes (`items_buffered` * 2048)
        estimated_memory: usize,
    },
}

/// Result type alias using fraiseql-wire [`WireError`]
pub type Result<T> = std::result::Result<T, WireError>;

impl WireError {
    /// Create a connection error with context
    pub fn connection<S: Into<String>>(msg: S) -> Self {
        WireError::Connection(msg.into())
    }

    /// Create a connection refused error (helpful message for debugging)
    pub fn connection_refused(host: &str, port: u16) -> Self {
        WireError::Connection(format!(
            "failed to connect to {}:{}: connection refused. \
            Is Postgres running? Verify with: pg_isready -h {} -p {}",
            host, port, host, port
        ))
    }

    /// Create a protocol error with context
    pub fn protocol<S: Into<String>>(msg: S) -> Self {
        WireError::Protocol(msg.into())
    }

    /// Create a SQL error with context
    pub fn sql<S: Into<String>>(msg: S) -> Self {
        WireError::Sql(msg.into())
    }

    /// Create a schema validation error (query returned wrong columns)
    pub fn invalid_schema_columns(num_columns: usize) -> Self {
        WireError::InvalidSchema(format!(
            "query returned {} columns instead of 1. \
            fraiseql-wire supports only: SELECT data FROM <view>. \
            See troubleshooting.md#error-invalid-result-schema",
            num_columns
        ))
    }

    /// Create an invalid schema error with context
    pub fn invalid_schema<S: Into<String>>(msg: S) -> Self {
        WireError::InvalidSchema(msg.into())
    }

    /// Create an authentication error with helpful message
    pub fn auth_failed(username: &str, reason: &str) -> Self {
        WireError::Authentication(format!(
            "authentication failed for user '{}': {}. \
            Verify credentials with: psql -U {} -W",
            username, reason, username
        ))
    }

    /// Create a config error with helpful message
    pub fn config_invalid<S: Into<String>>(msg: S) -> Self {
        WireError::Config(format!(
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
    pub const fn is_retriable(&self) -> bool {
        matches!(self, WireError::Io(_) | WireError::ConnectionClosed)
    }

    /// Get error category for observability and logging
    ///
    /// Used to categorize errors for metrics, tracing, and error handling decisions.
    pub const fn category(&self) -> &'static str {
        match self {
            WireError::Connection(_) => "connection",
            WireError::Authentication(_) => "authentication",
            WireError::Protocol(_) => "protocol",
            WireError::Sql(_) => "sql",
            WireError::JsonDecode(_) => "json_decode",
            WireError::Io(_) => "io",
            WireError::Config(_) => "config",
            WireError::Cancelled => "cancelled",
            WireError::InvalidSchema(_) => "invalid_schema",
            WireError::ConnectionBusy(_) => "connection_busy",
            WireError::InvalidState { .. } => "invalid_state",
            WireError::ConnectionClosed => "connection_closed",
            WireError::Deserialization { .. } => "deserialization",
            WireError::MemoryLimitExceeded { .. } => "memory_limit_exceeded",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_helpers() {
        let conn_err = WireError::connection("failed to connect");
        assert!(matches!(conn_err, WireError::Connection(_)));

        let proto_err = WireError::protocol("unexpected message");
        assert!(matches!(proto_err, WireError::Protocol(_)));

        let sql_err = WireError::sql("syntax error");
        assert!(matches!(sql_err, WireError::Sql(_)));

        let schema_err = WireError::invalid_schema("expected single column");
        assert!(matches!(schema_err, WireError::InvalidSchema(_)));
    }

    #[test]
    fn test_error_connection_refused() {
        let err = WireError::connection_refused("localhost", 5432);
        let msg = err.to_string();
        assert!(msg.contains("connection refused"));
        assert!(msg.contains("Is Postgres running?"));
        assert!(msg.contains("localhost"));
        assert!(msg.contains("5432"));
    }

    #[test]
    fn test_error_invalid_schema_columns() {
        let err = WireError::invalid_schema_columns(2);
        let msg = err.to_string();
        assert!(msg.contains("2 columns"));
        assert!(msg.contains("instead of 1"));
        assert!(msg.contains("SELECT data FROM"));
    }

    #[test]
    fn test_error_auth_failed() {
        let err = WireError::auth_failed("postgres", "invalid password");
        let msg = err.to_string();
        assert!(msg.contains("postgres"));
        assert!(msg.contains("invalid password"));
        assert!(msg.contains("psql"));
    }

    #[test]
    fn test_error_config_invalid() {
        let err = WireError::config_invalid("missing database name");
        let msg = err.to_string();
        assert!(msg.contains("invalid configuration"));
        assert!(msg.contains("postgres://"));
        assert!(msg.contains("missing database name"));
    }

    #[test]
    fn test_error_category() {
        assert_eq!(WireError::connection("test").category(), "connection");
        assert_eq!(WireError::sql("test").category(), "sql");
        assert_eq!(WireError::Cancelled.category(), "cancelled");
        assert_eq!(WireError::ConnectionClosed.category(), "connection_closed");
    }

    #[test]
    fn test_error_message_clarity() {
        // Verify error messages are clear and actionable
        let err = WireError::connection_refused("example.com", 5432);
        let msg = err.to_string();

        // Should suggest a diagnostic command
        assert!(msg.contains("pg_isready"));

        // Should include the connection details
        assert!(msg.contains("example.com"));
    }

    #[test]
    fn test_is_retriable() {
        assert!(WireError::ConnectionClosed.is_retriable());
        assert!(WireError::Io(io::Error::new(io::ErrorKind::TimedOut, "timeout")).is_retriable());

        assert!(!WireError::connection("test").is_retriable());
        assert!(!WireError::sql("test").is_retriable());
        assert!(!WireError::invalid_schema("test").is_retriable());
    }

    #[test]
    fn test_retriable_classification() {
        // Transient errors should be retriable
        assert!(WireError::ConnectionClosed.is_retriable());
        assert!(WireError::Io(io::Error::new(io::ErrorKind::ConnectionReset, "reset")).is_retriable());

        // Permanent errors should not be retriable
        assert!(!WireError::auth_failed("user", "invalid password").is_retriable());
        assert!(!WireError::sql("syntax error").is_retriable());
        assert!(!WireError::invalid_schema_columns(3).is_retriable());
    }

    #[test]
    fn test_deserialization_error() {
        let err = WireError::Deserialization {
            type_name: "Project".to_string(),
            details: "missing field `id`".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("Project"));
        assert!(msg.contains("missing field"));
        assert_eq!(err.category(), "deserialization");
    }

    #[test]
    fn test_deserialization_error_not_retriable() {
        let err = WireError::Deserialization {
            type_name: "User".to_string(),
            details: "invalid type".to_string(),
        };
        assert!(!err.is_retriable());
    }

    #[test]
    fn test_memory_limit_exceeded_error() {
        let err = WireError::MemoryLimitExceeded {
            limit: 1_000_000,
            estimated_memory: 1_500_000,
        };
        let msg = err.to_string();
        assert!(msg.contains("1500000"));
        assert!(msg.contains("1000000"));
        assert!(msg.contains("memory limit exceeded"));
        assert_eq!(err.category(), "memory_limit_exceeded");
    }

    #[test]
    fn test_memory_limit_exceeded_not_retriable() {
        let err = WireError::MemoryLimitExceeded {
            limit: 100_000,
            estimated_memory: 150_000,
        };
        assert!(!err.is_retriable());
    }
}
