//! Backend for reading secrets from environment variables

use chrono::{Duration, Utc};

use super::super::{SecretsBackend, SecretsError};

/// Secrets backend that reads from environment variables
///
/// Useful for local development and simple configurations
/// Not recommended for production credentials
///
/// # Example
/// ```ignore
/// std::env::set_var("DATABASE_PASSWORD", "secret123");
/// let backend = EnvBackend::new();
/// let secret = backend.get_secret("DATABASE_PASSWORD").await?;
/// // Returns: "secret123"
/// ```
///
/// # Note
/// Environment variables do not have expiry times. The backend returns a fixed 365-day
/// TTL for all environment variables to indicate they don't expire naturally.
#[derive(Clone, Debug)]
pub struct EnvBackend;

#[async_trait::async_trait]
impl SecretsBackend for EnvBackend {
    async fn get_secret(&self, name: &str) -> Result<String, SecretsError> {
        validate_secret_name(name)?;
        std::env::var(name)
            .map_err(|_| SecretsError::NotFound(format!("Environment variable {} not found", name)))
    }

    async fn get_secret_with_expiry(
        &self,
        name: &str,
    ) -> Result<(String, chrono::DateTime<Utc>), SecretsError> {
        let secret = self.get_secret(name).await?;
        // Environment variables don't expire, but we need to return an expiry
        // Use a long TTL (1 year) for env vars
        let expiry = Utc::now() + Duration::days(365);
        Ok((secret, expiry))
    }

    async fn rotate_secret(&self, name: &str) -> Result<String, SecretsError> {
        // Environment variables can't be rotated programmatically
        // Return error indicating rotation not supported
        Err(SecretsError::RotationError(format!(
            "Rotation not supported for environment variable {}",
            name
        )))
    }
}

impl EnvBackend {
    /// Create new EnvBackend
    pub fn new() -> Self {
        EnvBackend
    }
}

impl Default for EnvBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate secret name format
fn validate_secret_name(name: &str) -> Result<(), SecretsError> {
    if name.is_empty() {
        return Err(SecretsError::ValidationError("Secret name cannot be empty".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test EnvBackend reads from environment
    #[tokio::test]
    async fn test_env_backend_get_secret() {
        std::env::set_var("TEST_SECRET_KEY", "test_value_123");
        let backend = EnvBackend::new();

        let secret = backend.get_secret("TEST_SECRET_KEY").await.unwrap();
        assert_eq!(secret, "test_value_123");
    }

    /// Test EnvBackend returns error for missing variable
    #[tokio::test]
    async fn test_env_backend_not_found() {
        let backend = EnvBackend::new();
        let result = backend.get_secret("NONEXISTENT_VAR_XYZ").await;

        assert!(result.is_err());
        match result {
            Err(SecretsError::NotFound(_)) => {},
            _ => panic!("Expected NotFound error"),
        }
    }

    /// Test EnvBackend with_expiry returns future date
    #[tokio::test]
    async fn test_env_backend_with_expiry() {
        std::env::set_var("EXPIRY_TEST_KEY", "value");
        let backend = EnvBackend::new();

        let (secret, expiry) = backend.get_secret_with_expiry("EXPIRY_TEST_KEY").await.unwrap();
        assert_eq!(secret, "value");
        assert!(expiry > Utc::now(), "Expiry should be in future");
    }

    /// Test EnvBackend rotate returns error
    #[tokio::test]
    async fn test_env_backend_rotate_not_supported() {
        let backend = EnvBackend::new();
        let result = backend.rotate_secret("ANY_KEY").await;

        assert!(result.is_err());
        match result {
            Err(SecretsError::RotationError(_)) => {},
            _ => panic!("Expected RotationError"),
        }
    }

    /// Test empty environment variable
    #[tokio::test]
    async fn test_env_backend_empty_value() {
        std::env::set_var("EMPTY_VAR", "");
        let backend = EnvBackend::new();

        let secret = backend.get_secret("EMPTY_VAR").await.unwrap();
        assert_eq!(secret, "");
    }

    /// Test special characters in environment variable values
    #[tokio::test]
    async fn test_env_backend_special_chars() {
        let special_value = "p@$$w0rd!#$%^&*()";
        std::env::set_var("SPECIAL_VAR", special_value);
        let backend = EnvBackend::new();

        let secret = backend.get_secret("SPECIAL_VAR").await.unwrap();
        assert_eq!(secret, special_value);
    }

    /// Test multiple environment variables
    #[tokio::test]
    async fn test_env_backend_multiple_vars() {
        std::env::set_var("VAR1", "value1");
        std::env::set_var("VAR2", "value2");
        let backend = EnvBackend::new();

        let s1 = backend.get_secret("VAR1").await.unwrap();
        let s2 = backend.get_secret("VAR2").await.unwrap();

        assert_eq!(s1, "value1");
        assert_eq!(s2, "value2");
    }
}
