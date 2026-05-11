//! Backend for reading secrets from environment variables

use chrono::{Duration, Utc};

use super::super::{SecretsBackend, SecretsError};

/// Secrets backend that reads from environment variables
///
/// Useful for local development and simple configurations
/// Not recommended for production credentials
///
/// # Example
/// ```no_run
/// // Requires: DATABASE_PASSWORD environment variable to be set.
/// # async fn example() -> Result<(), fraiseql_secrets::secrets_manager::SecretsError> {
/// use fraiseql_secrets::secrets_manager::EnvBackend;
/// use fraiseql_secrets::secrets_manager::SecretsBackend;
/// std::env::set_var("DATABASE_PASSWORD", "secret123");
/// let backend = EnvBackend::new();
/// let secret = backend.get_secret("DATABASE_PASSWORD").await?;
/// // Returns: "secret123"
/// # Ok(())
/// # }
/// ```
///
/// # Note
/// Environment variables do not have expiry times. The backend returns a fixed 365-day
/// TTL for all environment variables to indicate they don't expire naturally.
#[derive(Clone, Debug)]
pub struct EnvBackend;

#[async_trait::async_trait]
impl SecretsBackend for EnvBackend {
    fn name(&self) -> &'static str {
        "env"
    }

    async fn health_check(&self) -> Result<(), SecretsError> {
        Ok(()) // Environment variables are always available
    }

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
    /// Create new `EnvBackend`
    pub const fn new() -> Self {
        EnvBackend
    }
}

impl Default for EnvBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate secret name format.
///
/// Accepts only names matching the POSIX environment variable charset
/// `[A-Z_][A-Z0-9_]*` (case-insensitive first letter also accepted for
/// portability). Rejects `=` and NUL bytes, which are OS-undefined.
fn validate_secret_name(name: &str) -> Result<(), SecretsError> {
    if name.is_empty() {
        return Err(SecretsError::ValidationError("Secret name cannot be empty".to_string()));
    }
    let mut chars = name.chars();
    let first = chars.next().expect("non-empty; checked above");
    if !first.is_ascii_alphabetic() && first != '_' {
        return Err(SecretsError::ValidationError(format!(
            "Secret name '{name}' must start with a letter or underscore (POSIX env var charset)"
        )));
    }
    for ch in chars {
        if !ch.is_ascii_alphanumeric() && ch != '_' {
            return Err(SecretsError::ValidationError(format!(
                "Secret name '{name}' contains invalid character '{ch}' \
                 (only [A-Za-z0-9_] allowed)"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
