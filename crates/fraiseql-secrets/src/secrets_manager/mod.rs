//! Abstraction layer for multiple secrets backends (Vault, Environment Variables, File)
//!
//! This module provides a unified interface to manage secrets from different sources:
//! - HashiCorp Vault for dynamic credentials
//! - Environment variables for configuration
//! - Local files for development/testing

use std::{fmt, path::PathBuf, sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use tracing::{info, warn};

pub mod backends;
pub mod types;

pub use backends::{EnvBackend, FileBackend, VaultBackend};
pub use types::{Secret, SecretsBackend};

/// Configuration for selecting and initializing a secrets backend.
#[derive(Debug, Clone)]
pub enum SecretsBackendConfig {
    /// Read secrets from local files (development/testing).
    File {
        /// Base directory containing secret files.
        path: PathBuf,
    },
    /// Read secrets from environment variables.
    Env,
    /// Read secrets from HashiCorp Vault.
    Vault {
        /// Vault server address (e.g., `https://vault.example.com:8200`).
        addr:       String,
        /// Authentication method.
        auth:       VaultAuth,
        /// Optional namespace (Enterprise feature).
        namespace:  Option<String>,
        /// Whether to verify TLS certificates.
        tls_verify: bool,
    },
}

/// Vault authentication methods.
#[derive(Debug, Clone)]
pub enum VaultAuth {
    /// Authenticate with a static token.
    Token(String),
    /// Authenticate via AppRole (recommended for production).
    AppRole {
        /// The role ID for AppRole login.
        role_id:   String,
        /// The secret ID for AppRole login.
        secret_id: String,
    },
}

/// Create a `SecretsManager` from configuration.
///
/// # Errors
///
/// Returns `SecretsError` if the backend cannot be initialized (e.g., Vault
/// AppRole login fails).
pub async fn create_secrets_manager(
    config: SecretsBackendConfig,
) -> Result<Arc<SecretsManager>, SecretsError> {
    let backend: Arc<dyn SecretsBackend> = match config {
        SecretsBackendConfig::File { path } => {
            info!(path = %path.display(), "Initializing file secrets backend");
            Arc::new(FileBackend::new(path))
        },
        SecretsBackendConfig::Env => {
            info!("Initializing environment variable secrets backend");
            Arc::new(EnvBackend::new())
        },
        SecretsBackendConfig::Vault {
            addr,
            auth,
            namespace,
            tls_verify,
        } => {
            info!(addr = %addr, "Initializing Vault secrets backend");
            let mut vault = match auth {
                VaultAuth::Token(token) => VaultBackend::new(&addr, &token),
                VaultAuth::AppRole { role_id, secret_id } => {
                    VaultBackend::with_approle(&addr, &role_id, &secret_id).await?
                },
            };
            if let Some(ns) = namespace {
                vault = vault.with_namespace(ns);
            }
            vault = vault.with_tls_verify(tls_verify);
            Arc::new(vault)
        },
    };
    Ok(Arc::new(SecretsManager::new(backend)))
}

/// Primary secrets manager that caches and rotates credentials.
pub struct SecretsManager {
    backend: Arc<dyn SecretsBackend>,
}

impl SecretsManager {
    /// Create new `SecretsManager` with specified backend.
    pub fn new(backend: Arc<dyn SecretsBackend>) -> Self {
        SecretsManager { backend }
    }

    /// Get secret by name from backend.
    pub async fn get_secret(&self, name: &str) -> Result<String, SecretsError> {
        self.backend.get_secret(name).await
    }

    /// Get secret with expiry time.
    ///
    /// Returns tuple of (secret_value, expiry_datetime).
    /// Useful for dynamic credentials with lease durations.
    pub async fn get_secret_with_expiry(
        &self,
        name: &str,
    ) -> Result<(String, DateTime<Utc>), SecretsError> {
        self.backend.get_secret_with_expiry(name).await
    }

    /// Rotate secret to new value.
    ///
    /// For backends that support it (e.g., Vault), generates new credential.
    pub async fn rotate_secret(&self, name: &str) -> Result<String, SecretsError> {
        self.backend.rotate_secret(name).await
    }
}

/// Background task that renews expiring Vault leases.
///
/// Periodically checks cached secrets and refreshes those approaching expiry
/// (within 20% of their original TTL). Designed to run as a background tokio task.
pub struct LeaseRenewalTask {
    manager:        Arc<SecretsManager>,
    check_interval: Duration,
    cancel_rx:      tokio::sync::watch::Receiver<bool>,
    tracked_keys:   Vec<String>,
}

impl LeaseRenewalTask {
    /// Create a new lease renewal task.
    ///
    /// Returns the task and a sender to trigger cancellation (send `true` to stop).
    pub fn new(
        manager: Arc<SecretsManager>,
        tracked_keys: Vec<String>,
        check_interval: Duration,
    ) -> (Self, tokio::sync::watch::Sender<bool>) {
        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        (
            Self {
                manager,
                check_interval,
                cancel_rx,
                tracked_keys,
            },
            cancel_tx,
        )
    }

    /// Run the lease renewal loop.
    ///
    /// Blocks until the cancel sender sends `true` or is dropped.
    pub async fn run(mut self) {
        info!(
            interval_secs = self.check_interval.as_secs(),
            keys = self.tracked_keys.len(),
            "Lease renewal task started"
        );
        loop {
            tokio::select! {
                result = self.cancel_rx.changed() => {
                    if result.is_err() || *self.cancel_rx.borrow() {
                        info!("Lease renewal task stopped");
                        break;
                    }
                },
                () = tokio::time::sleep(self.check_interval) => {
                    self.renew_expiring_leases().await;
                }
            }
        }
    }

    async fn renew_expiring_leases(&self) {
        for key in &self.tracked_keys {
            match self.manager.get_secret_with_expiry(key).await {
                Ok((_, expiry)) => {
                    let remaining = expiry - Utc::now();
                    // Refresh if less than 20% of the check interval remains
                    if remaining
                        < chrono::Duration::seconds(
                            (self.check_interval.as_secs() as f64 * 0.2) as i64,
                        )
                    {
                        match self.manager.rotate_secret(key).await {
                            Ok(_) => info!(key = %key, "Lease renewed"),
                            Err(e) => warn!(key = %key, error = %e, "Lease renewal failed"),
                        }
                    }
                },
                Err(e) => {
                    warn!(key = %key, error = %e, "Failed to check lease expiry");
                },
            }
        }
    }
}

/// Error type for secrets operations.
#[derive(Debug, Clone)]
pub enum SecretsError {
    /// Secret not found in the backend.
    NotFound(String),
    /// Backend communication or configuration error.
    BackendError(String),
    /// Invalid input (e.g., bad secret name format).
    ValidationError(String),
    /// Encryption or decryption failure.
    EncryptionError(String),
    /// Rotation not supported or failed.
    RotationError(String),
    /// Connection error (e.g., Vault unreachable).
    ConnectionError(String),
    /// Credential has expired.
    ExpiredCredential,
}

impl fmt::Display for SecretsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SecretsError::NotFound(msg) => write!(f, "Secret not found: {msg}"),
            SecretsError::BackendError(msg) => write!(f, "Backend error: {msg}"),
            SecretsError::ValidationError(msg) => write!(f, "Validation error: {msg}"),
            SecretsError::EncryptionError(msg) => write!(f, "Encryption error: {msg}"),
            SecretsError::RotationError(msg) => write!(f, "Rotation error: {msg}"),
            SecretsError::ConnectionError(msg) => write!(f, "Connection error: {msg}"),
            SecretsError::ExpiredCredential => write!(f, "Credential expired"),
        }
    }
}

impl std::error::Error for SecretsError {}

#[allow(clippy::unwrap_used)]  // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_secrets_manager_file_backend() {
        let dir = tempfile::tempdir().unwrap();
        let secret_path = dir.path().join("db_password");
        tokio::fs::write(&secret_path, "s3cret").await.unwrap();

        let manager = create_secrets_manager(SecretsBackendConfig::File {
            path: dir.path().to_path_buf(),
        })
        .await
        .unwrap();

        let value = manager.get_secret("db_password").await.unwrap();
        assert_eq!(value, "s3cret");
    }

    #[tokio::test]
    async fn test_create_secrets_manager_env_backend() {
        // Use a unique env var to avoid test interference
        let key = "FRAISEQL_TEST_SM_SECRET_FACTORY";
        temp_env::async_with_vars([(key, Some("env_value"))], async {
            let manager = create_secrets_manager(SecretsBackendConfig::Env).await.unwrap();
            let value = manager.get_secret(key).await.unwrap();
            assert_eq!(value, "env_value");
        })
        .await;
    }
}
