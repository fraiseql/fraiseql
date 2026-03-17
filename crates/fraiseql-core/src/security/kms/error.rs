//! KMS-specific error types.

use std::fmt;

/// KMS operation errors.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum KmsError {
    /// Key not found in KMS provider
    KeyNotFound {
        /// Identifier of the missing key.
        key_id: String,
    },
    /// Encryption operation failed
    EncryptionFailed {
        /// Human-readable description of the failure.
        message: String,
    },
    /// Decryption operation failed
    DecryptionFailed {
        /// Human-readable description of the failure.
        message: String,
    },
    /// Key rotation failed
    RotationFailed {
        /// Human-readable description of the failure.
        message: String,
    },
    /// Connection to KMS provider failed
    ProviderConnectionError {
        /// Human-readable description of the connection error.
        message: String,
    },
    /// Invalid configuration
    InvalidConfiguration {
        /// Human-readable description of what is misconfigured.
        message: String,
    },
    /// Serialization/deserialization error
    SerializationError {
        /// Human-readable description of the serialization failure.
        message: String,
    },
}

impl fmt::Display for KmsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KeyNotFound { key_id } => write!(f, "Key not found: {}", key_id),
            Self::EncryptionFailed { message } => write!(f, "Encryption failed: {}", message),
            Self::DecryptionFailed { message } => write!(f, "Decryption failed: {}", message),
            Self::RotationFailed { message } => write!(f, "Key rotation failed: {}", message),
            Self::ProviderConnectionError { message } => {
                write!(f, "Provider connection error: {}", message)
            },
            Self::InvalidConfiguration { message } => {
                write!(f, "Invalid configuration: {}", message)
            },
            Self::SerializationError { message } => {
                write!(f, "Serialization error: {}", message)
            },
        }
    }
}

impl std::error::Error for KmsError {}

/// Convenience `Result` alias for KMS operations.
pub type KmsResult<T> = Result<T, KmsError>;
