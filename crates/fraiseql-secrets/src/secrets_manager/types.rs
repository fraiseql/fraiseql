//! Core types for secrets management

use std::fmt;

use chrono::{DateTime, Utc};

use super::SecretsError;

/// Trait for different secrets backends
///
/// Implementations: Vault, Environment Variables, File-based
#[async_trait::async_trait]
pub trait SecretsBackend: Send + Sync {
    /// Returns the backend type name (e.g., `"vault"`, `"env"`, `"file"`).
    fn name(&self) -> &'static str;

    /// Performs a lightweight connectivity check.
    ///
    /// Returns `Ok(())` if the backend is reachable and authenticated.
    async fn health_check(&self) -> Result<(), SecretsError>;

    /// Get secret by name
    ///
    /// # Arguments
    /// * `name` - Secret identifier (path, env var name, etc.)
    ///
    /// # Returns
    /// Secret value as String, or `SecretsError` if not found/error
    async fn get_secret(&self, name: &str) -> Result<String, SecretsError>;

    /// Get secret with expiry information
    ///
    /// Useful for dynamic credentials from Vault with lease durations
    ///
    /// # Returns
    /// Tuple of (`secret_value`, `expiry_datetime`)
    async fn get_secret_with_expiry(
        &self,
        name: &str,
    ) -> Result<(String, DateTime<Utc>), SecretsError>;

    /// Rotate secret to new value
    ///
    /// For backends supporting rotation (Vault), generates new credential
    /// For static backends (env, file), may be no-op or return error
    async fn rotate_secret(&self, name: &str) -> Result<String, SecretsError>;
}

/// Wrapper for secrets that redacts values in logs/debug output
///
/// Prevents accidental secret exposure through string formatting
///
/// # Example
/// ```rust
/// use fraiseql_secrets::secrets_manager::Secret;
/// let secret = Secret::new("password123".to_string());
/// println!("{:?}", secret);  // Prints: Secret(***)
/// let actual = secret.expose();  // Returns: "password123"
/// assert_eq!(actual, "password123");
/// ```
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    /// Create new Secret wrapper
    #[must_use]
    pub const fn new(value: String) -> Self {
        Secret(value)
    }

    /// Expose the actual secret value
    ///
    /// Should only be called when actually using the secret
    /// Not called in logging or debugging code
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Convert to owned String (consumes Secret)
    #[must_use]
    pub fn into_exposed(self) -> String {
        self.0
    }

    /// Check if secret is empty
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Get length of secret
    #[must_use]
    pub const fn len(&self) -> usize {
        self.0.len()
    }
}

/// Debug output redacts actual secret value
impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Secret(***)")
    }
}

/// Display output redacts actual secret value
impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "***")
    }
}

/// Partial equality that compares actual values (for testing)
impl PartialEq for Secret {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Secret {}

#[cfg(test)]
mod tests;
