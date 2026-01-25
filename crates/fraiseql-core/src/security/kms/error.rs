//! KMS-specific error types.

use std::fmt;

/// KMS operation errors.
#[derive(Debug, Clone)]
pub enum KmsError {
    /// Key not found in KMS provider
    KeyNotFound { key_id: String },
    /// Encryption operation failed
    EncryptionFailed { message: String },
    /// Decryption operation failed
    DecryptionFailed { message: String },
    /// Key rotation failed
    RotationFailed { message: String },
    /// Connection to KMS provider failed
    ProviderConnectionError { message: String },
    /// Invalid configuration
    InvalidConfiguration { message: String },
    /// Serialization/deserialization error
    SerializationError { message: String },
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
            }
            Self::InvalidConfiguration { message } => {
                write!(f, "Invalid configuration: {}", message)
            }
            Self::SerializationError { message } => {
                write!(f, "Serialization error: {}", message)
            }
        }
    }
}

impl std::error::Error for KmsError {}

pub type KmsResult<T> = Result<T, KmsError>;
