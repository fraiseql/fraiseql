//! Abstraction layer for multiple secrets backends (Vault, Environment Variables, File)
//!
//! This module provides a unified interface to manage secrets from different sources:
//! - HashiCorp Vault for dynamic credentials
//! - Environment variables for configuration
//! - Local files for development/testing

use std::{fmt, sync::Arc};

use chrono::{DateTime, Utc};

pub mod backends;
pub mod types;

pub use backends::{EnvBackend, FileBackend, VaultBackend};
pub use types::{Secret, SecretsBackend};

/// Primary secrets manager that caches and rotates credentials
pub struct SecretsManager {
    backend: Arc<dyn SecretsBackend>,
}

impl SecretsManager {
    /// Create new SecretsManager with specified backend
    pub fn new(backend: Arc<dyn SecretsBackend>) -> Self {
        SecretsManager { backend }
    }

    /// Get secret by name from backend
    pub async fn get_secret(&self, name: &str) -> Result<String, SecretsError> {
        self.backend.get_secret(name).await
    }

    /// Get secret with expiry time
    ///
    /// Returns tuple of (secret_value, expiry_datetime)
    /// Useful for dynamic credentials with lease durations
    pub async fn get_secret_with_expiry(
        &self,
        name: &str,
    ) -> Result<(String, DateTime<Utc>), SecretsError> {
        self.backend.get_secret_with_expiry(name).await
    }

    /// Rotate secret to new value
    ///
    /// For backends that support it (e.g., Vault), generates new credential
    pub async fn rotate_secret(&self, name: &str) -> Result<String, SecretsError> {
        self.backend.rotate_secret(name).await
    }
}

/// Error type for secrets operations
#[derive(Debug, Clone)]
pub enum SecretsError {
    NotFound(String),
    BackendError(String),
    ValidationError(String),
    EncryptionError(String),
    RotationError(String),
    ExpiredCredential,
}

impl fmt::Display for SecretsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SecretsError::NotFound(msg) => write!(f, "Secret not found: {}", msg),
            SecretsError::BackendError(msg) => write!(f, "Backend error: {}", msg),
            SecretsError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            SecretsError::EncryptionError(msg) => write!(f, "Encryption error: {}", msg),
            SecretsError::RotationError(msg) => write!(f, "Rotation error: {}", msg),
            SecretsError::ExpiredCredential => write!(f, "Credential expired"),
        }
    }
}

impl std::error::Error for SecretsError {}
