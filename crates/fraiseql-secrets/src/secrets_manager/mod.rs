//! Abstraction layer for multiple secrets backends (Vault, Environment Variables, File)
//!
//! This module provides a unified interface to manage secrets from different sources:
//! - HashiCorp Vault for dynamic credentials
//! - Environment variables for configuration
//! - Local files for development/testing

use std::{fmt, path::PathBuf, sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use tracing::{info, warn};
use zeroize::Zeroizing;

pub mod backends;
pub mod types;

pub use backends::{EnvBackend, FileBackend, VaultBackend};
pub use types::{Secret, SecretsBackend};

/// Configuration for selecting and initializing a secrets backend.
#[derive(Debug, Clone)]
#[non_exhaustive]
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
///
/// Sensitive fields (`Token` payload and `secret_id`) are wrapped in
/// [`Zeroizing`] so that the credential bytes are overwritten on drop rather
/// than remaining in heap until the allocator reuses the memory.
#[derive(Clone)]
#[non_exhaustive]
pub enum VaultAuth {
    /// Authenticate with a static token.
    Token(Zeroizing<String>),
    /// Authenticate via AppRole (recommended for production).
    AppRole {
        /// The role ID for AppRole login.
        role_id:   String,
        /// The secret ID for AppRole login (high-value credential).
        secret_id: Zeroizing<String>,
    },
}

impl fmt::Debug for VaultAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Token(_) => f.debug_tuple("Token").field(&"[REDACTED]").finish(),
            Self::AppRole { role_id, .. } => f
                .debug_struct("AppRole")
                .field("role_id", role_id)
                .field("secret_id", &"[REDACTED]")
                .finish(),
        }
    }
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
                VaultAuth::Token(token) => VaultBackend::new(addr.as_str(), token.as_str())?,
                VaultAuth::AppRole { role_id, secret_id } => {
                    VaultBackend::with_approle(&addr, &role_id, secret_id.as_str()).await?
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

    /// Returns the backend type name (e.g., `"vault"`, `"env"`, `"file"`).
    pub fn backend_name(&self) -> &'static str {
        self.backend.name()
    }

    /// Performs a lightweight connectivity check on the underlying backend.
    ///
    /// # Errors
    ///
    /// Returns [`SecretsError`] if the backend is unreachable or returns an error.
    pub async fn health_check(&self) -> Result<(), SecretsError> {
        self.backend.health_check().await
    }

    /// Get secret by name from backend.
    ///
    /// # Errors
    ///
    /// Returns [`SecretsError`] if the secret does not exist or the backend returns an error.
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
    ///
    /// # Errors
    ///
    /// Returns [`SecretsError`] if rotation is unsupported by the backend or the backend returns an error.
    pub async fn rotate_secret(&self, name: &str) -> Result<String, SecretsError> {
        self.backend.rotate_secret(name).await
    }
}

/// Background task that renews expiring Vault leases.
///
/// Periodically checks cached secrets and refreshes those approaching expiry
/// (within one `check_interval` of expiry, ensuring renewal before the next
/// poll cycle can catch it). Designed to run as a background tokio task.
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
                    // Refresh if less than one full check interval remains,
                    // ensuring renewal completes before the next poll would be too late.
                    if remaining < chrono::Duration::seconds(self.check_interval.as_secs() as i64) {
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
#[non_exhaustive]
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

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };

    use chrono::Utc;

    use super::*;

    // ---------------------------------------------------------------------------
    // Mock SecretsBackend for LeaseRenewalTask tests
    // ---------------------------------------------------------------------------

    /// A mock backend that returns a fixed secret with configurable expiry.
    /// Rotation calls are counted so tests can assert how many renewals occurred.
    struct MockBackend {
        secret:       String,
        expiry:       DateTime<Utc>,
        rotate_count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl SecretsBackend for MockBackend {
        fn name(&self) -> &'static str {
            "mock"
        }

        async fn health_check(&self) -> Result<(), SecretsError> {
            Ok(())
        }

        async fn get_secret(&self, _name: &str) -> Result<String, SecretsError> {
            Ok(self.secret.clone())
        }

        async fn get_secret_with_expiry(
            &self,
            _name: &str,
        ) -> Result<(String, DateTime<Utc>), SecretsError> {
            Ok((self.secret.clone(), self.expiry))
        }

        async fn rotate_secret(&self, _name: &str) -> Result<String, SecretsError> {
            self.rotate_count.fetch_add(1, Ordering::SeqCst);
            Ok("rotated".to_string())
        }
    }

    // ---------------------------------------------------------------------------
    // LeaseRenewalTask tests
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_lease_renewal_task_cancels_cleanly() {
        let rotate_count = Arc::new(AtomicUsize::new(0));
        let backend = MockBackend {
            secret:       "s3cret".to_string(),
            // Expiry far in the future — no renewal needed.
            expiry:       Utc::now() + chrono::Duration::hours(1),
            rotate_count: Arc::clone(&rotate_count),
        };
        let manager = Arc::new(SecretsManager::new(Arc::new(backend)));

        let (task, cancel_tx) =
            LeaseRenewalTask::new(manager, vec!["db/creds".to_string()], Duration::from_secs(60));

        // Cancel immediately before the first sleep interval fires.
        cancel_tx.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(2), task.run())
            .await
            .expect("task should exit quickly after cancellation");

        // No renewals should have occurred since we cancelled before the first tick.
        assert_eq!(rotate_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_lease_renewal_triggers_rotate_when_expiry_near() {
        let rotate_count = Arc::new(AtomicUsize::new(0));
        let backend = MockBackend {
            secret:       "s3cret".to_string(),
            // Already-expired credential: remaining is negative, which is always
            // less than the check_interval threshold, so renewal fires on every tick.
            // This works with any sub-second check_interval (where as_secs() == 0)
            // because negative < zero is true for chrono::Duration.
            expiry:       Utc::now() - chrono::Duration::seconds(1),
            rotate_count: Arc::clone(&rotate_count),
        };
        let manager = Arc::new(SecretsManager::new(Arc::new(backend)));

        let (task, cancel_tx) = LeaseRenewalTask::new(
            manager,
            vec!["db/creds".to_string()],
            Duration::from_millis(50), // very short interval so the test is fast
        );

        let handle = tokio::spawn(task.run());

        // Wait long enough for at least one tick to fire.
        tokio::time::sleep(Duration::from_millis(200)).await;
        cancel_tx.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("task should exit quickly after cancellation")
            .unwrap();

        assert!(
            rotate_count.load(Ordering::SeqCst) >= 1,
            "expected at least one renewal for an expired credential"
        );
    }

    #[tokio::test]
    async fn test_lease_renewal_skips_non_expiring_keys() {
        let rotate_count = Arc::new(AtomicUsize::new(0));
        let backend = MockBackend {
            secret:       "s3cret".to_string(),
            // Expiry 1 hour away — much longer than the check interval (50 ms).
            expiry:       Utc::now() + chrono::Duration::hours(1),
            rotate_count: Arc::clone(&rotate_count),
        };
        let manager = Arc::new(SecretsManager::new(Arc::new(backend)));

        let (task, cancel_tx) =
            LeaseRenewalTask::new(manager, vec!["db/creds".to_string()], Duration::from_millis(50));

        let handle = tokio::spawn(task.run());
        tokio::time::sleep(Duration::from_millis(200)).await;
        cancel_tx.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("task should exit quickly")
            .unwrap();

        assert_eq!(
            rotate_count.load(Ordering::SeqCst),
            0,
            "credentials with distant expiry should not be rotated"
        );
    }

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
