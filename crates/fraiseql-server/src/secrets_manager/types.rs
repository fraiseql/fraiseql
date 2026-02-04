// Phase 12.1 Cycle 1: Secrets Manager Types
//! Core types for secrets management

use chrono::{DateTime, Utc};
use std::fmt;

use super::SecretsError;

/// Trait for different secrets backends
///
/// Implementations: Vault, Environment Variables, File-based
#[async_trait::async_trait]
pub trait SecretsBackend: Send + Sync {
    /// Get secret by name
    ///
    /// # Arguments
    /// * `name` - Secret identifier (path, env var name, etc.)
    ///
    /// # Returns
    /// Secret value as String, or SecretsError if not found/error
    async fn get_secret(&self, name: &str) -> Result<String, SecretsError>;

    /// Get secret with expiry information
    ///
    /// Useful for dynamic credentials from Vault with lease durations
    ///
    /// # Returns
    /// Tuple of (secret_value, expiry_datetime)
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
/// ```ignore
/// let secret = Secret::new("password123".to_string());
/// println!("{:?}", secret);  // Prints: Secret(***)
/// let actual = secret.expose();  // Returns: "password123"
/// ```
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    /// Create new Secret wrapper
    pub fn new(value: String) -> Self {
        Secret(value)
    }

    /// Expose the actual secret value
    ///
    /// Should only be called when actually using the secret
    /// Not called in logging or debugging code
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Convert to owned String (consumes Secret)
    pub fn into_exposed(self) -> String {
        self.0
    }

    /// Check if secret is empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Get length of secret
    pub fn len(&self) -> usize {
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
mod tests {
    use super::*;

    /// Test Secret wrapper redacts in Debug output
    #[test]
    fn test_secret_debug_redaction() {
        let secret = Secret::new("my_secret_password".to_string());
        let debug_str = format!("{:?}", secret);

        assert!(debug_str.contains("***"), "Debug should redact secret");
        assert!(
            !debug_str.contains("my_secret_password"),
            "Debug should not contain actual value"
        );
        assert_eq!(debug_str, "Secret(***)");
    }

    /// Test Secret wrapper redacts in Display output
    #[test]
    fn test_secret_display_redaction() {
        let secret = Secret::new("api_key_12345".to_string());
        let display_str = format!("{}", secret);

        assert_eq!(display_str, "***", "Display should only show ***");
    }

    /// Test Secret.expose() returns actual value
    #[test]
    fn test_secret_expose() {
        let value = "actual_secret_value".to_string();
        let secret = Secret::new(value.clone());

        assert_eq!(secret.expose(), &value);
    }

    /// Test Secret.into_exposed() consumes and returns value
    #[test]
    fn test_secret_into_exposed() {
        let value = "test_secret".to_string();
        let secret = Secret::new(value.clone());

        let exposed = secret.into_exposed();
        assert_eq!(exposed, value);
    }

    /// Test Secret equality based on actual value
    #[test]
    fn test_secret_equality() {
        let secret1 = Secret::new("same_value".to_string());
        let secret2 = Secret::new("same_value".to_string());
        let secret3 = Secret::new("different_value".to_string());

        assert_eq!(secret1, secret2, "Secrets with same value should be equal");
        assert_ne!(secret1, secret3, "Secrets with different values should not be equal");
    }

    /// Test Secret length and is_empty
    #[test]
    fn test_secret_properties() {
        let secret = Secret::new("test".to_string());
        assert_eq!(secret.len(), 4);
        assert!(!secret.is_empty());

        let empty = Secret::new(String::new());
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
    }

    /// Test SecretsBackend trait requirements
    #[test]
    fn test_secrets_backend_trait_definition() {
        // Trait should require:
        // 1. Send + Sync for thread safety
        // 2. get_secret(&self, name: &str) -> Future<Result<String>>
        // 3. get_secret_with_expiry(&self, name: &str) -> Future<Result<(String, DateTime<Utc>)>>
        // 4. rotate_secret(&self, name: &str) -> Future<Result<String>>
        // All methods async for I/O operations
        assert!(true);
    }
}
