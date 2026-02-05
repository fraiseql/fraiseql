//! Authentication mechanisms for fraiseql-wire
//!
//! Supports SCRAM-SHA-256 (Postgres 10+) as the primary authentication method.

pub mod scram;

pub use scram::{ScramClient, ScramError};

use std::fmt;

/// Authentication error types
#[derive(Debug, Clone)]
pub enum AuthError {
    /// SCRAM-specific error
    Scram(ScramError),
    /// Server doesn't support required mechanism
    MechanismNotSupported(String),
    /// Invalid server message format
    InvalidServerMessage(String),
    /// UTF-8 encoding error
    Utf8Error(String),
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthError::Scram(e) => write!(f, "SCRAM authentication error: {}", e),
            AuthError::MechanismNotSupported(mech) => {
                write!(f, "server does not support mechanism: {}", mech)
            }
            AuthError::InvalidServerMessage(msg) => {
                write!(f, "invalid server message format: {}", msg)
            }
            AuthError::Utf8Error(msg) => write!(f, "UTF-8 encoding error: {}", msg),
        }
    }
}

impl std::error::Error for AuthError {}

impl From<ScramError> for AuthError {
    fn from(e: ScramError) -> Self {
        AuthError::Scram(e)
    }
}
