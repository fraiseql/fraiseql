//! Error types for Arrow Flight operations.

use thiserror::Error;

/// Errors specific to Arrow Flight operations.
#[derive(Debug, Error)]
pub enum ArrowFlightError {
    /// Arrow library error
    #[error("Arrow error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),

    /// Flight protocol error
    #[error("Flight error: {0}")]
    Flight(String),

    /// Invalid ticket format or content
    #[error("Invalid ticket: {0}")]
    InvalidTicket(String),

    /// Schema not found for the requested resource
    #[error("Schema not found: {0}")]
    SchemaNotFound(String),

    /// gRPC transport error (boxed to reduce enum size)
    #[error("Transport error: {0}")]
    Transport(Box<tonic::Status>),

    /// JSON serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Result type for Arrow Flight operations
pub type Result<T> = std::result::Result<T, ArrowFlightError>;

impl From<tonic::Status> for ArrowFlightError {
    fn from(status: tonic::Status) -> Self {
        Self::Transport(Box::new(status))
    }
}
