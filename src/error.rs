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
